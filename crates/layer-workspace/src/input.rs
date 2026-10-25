use crate::{MAX_PACKAGE_BYTES,StoreError};
use serde::de::DeserializeOwned;
use serde_json::Value;

pub(crate) const MAX_INPUT_NODES:usize=layer_core::package::MAX_JSON_NODES;
pub(crate) struct InputBudget {pub bytes:usize,nodes:Option<usize>}
impl Default for InputBudget {
    fn default()->Self {Self {bytes:MAX_PACKAGE_BYTES,nodes:Some(MAX_INPUT_NODES)}}
}
impl InputBudget {
    pub fn stored()->Self {Self {bytes:MAX_PACKAGE_BYTES,nodes:None}}
    pub fn json(&mut self,text:&str)->Result<Value,StoreError> {
        self.bytes=self.bytes.checked_sub(text.len()).ok_or_else(limit)?;
        match &mut self.nodes {
            Some(nodes)=>layer_core::package::parse_json_with_budget(text.as_bytes(),MAX_PACKAGE_BYTES,nodes)
                .map_err(|error|StoreError::invalid(error.to_string())),
            None=>Ok(serde_json::from_str(text)?),
        }
    }
}
pub(crate) fn limit()->StoreError {StoreError::invalid("Workspace input exceeds supported limits.")}
pub(crate) fn stored_json<T:DeserializeOwned>(text:&str)->Result<T,StoreError> {
    if text.len()>MAX_PACKAGE_BYTES {return Err(limit());}
    Ok(serde_json::from_str(text)?)
}
pub(crate) fn copy(text:&str,remaining:usize)->Result<String,StoreError> {
    if text.len()>remaining {return Err(limit());}
    let mut result=String::new();result.try_reserve_exact(text.len()).map_err(|_|limit())?;
    result.push_str(text);Ok(result)
}
struct Output {bytes:Vec<u8>,limit:usize}
impl std::io::Write for Output {
    fn write(&mut self,bytes:&[u8])->std::io::Result<usize> {
        let end=self.bytes.len().checked_add(bytes.len()).filter(|end|*end<=self.limit)
            .ok_or_else(||std::io::Error::other(limit()))?;
        if end>self.bytes.capacity() {
            let capacity=end.max(self.bytes.capacity().saturating_mul(2)).min(self.limit);
            self.bytes.try_reserve_exact(capacity-self.bytes.len()).map_err(|error|std::io::Error::other(error.to_string()))?;
        }
        self.bytes.extend_from_slice(bytes);Ok(bytes.len())
    }
    fn flush(&mut self)->std::io::Result<()> {Ok(())}
}
pub(crate) fn encoded(value:&impl serde::Serialize)->Result<String,StoreError> {
    let mut output=Output {bytes:Vec::new(),limit:MAX_PACKAGE_BYTES};
    serde_json::to_writer(&mut output,value)?;
    layer_core::package::json_node_count(&output.bytes,MAX_PACKAGE_BYTES).map_err(|error|StoreError::invalid(error.to_string()))?;
    String::from_utf8(output.bytes).map_err(|error|StoreError::invalid(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn output_admission_does_not_append_a_chunk_beyond_the_limit() {
        let mut output=Output {bytes:Vec::new(),limit:7};
        output.write_all(b"[1,2").unwrap();assert!(output.write_all(b",3,4]").is_err());
        assert_eq!(output.bytes,b"[1,2");
        output.write_all(b",3]").unwrap();assert_eq!(output.bytes,b"[1,2,3]");
    }

    #[test]
    fn repeated_component_expansions_share_one_budget_before_copying() {
        let layout=layer_ui::DockLayout::default();
        let entity=crate::Entity::workspace("Budget",layer_ui::WorkspaceCapture::from_template(&layout).unwrap(),layout,0);
        let mut components=std::collections::BTreeMap::new();
        let root=crate::protocol::pack(serde_json::to_value(entity.content).unwrap(),&mut components).unwrap().to_string();
        let read=|id:&str,remaining|copy(&components[id],remaining);
        let mut measured=InputBudget::default();
        crate::protocol::unpack_with_budget(&root,read,&mut measured).unwrap();
        let used=MAX_PACKAGE_BYTES-measured.bytes;assert!(used>root.len());
        let mut budget=InputBudget {bytes:used*2-1,nodes:Some(MAX_INPUT_NODES)};
        crate::protocol::unpack_with_budget(&root,read,&mut budget).unwrap();
        assert!(crate::protocol::unpack_with_budget(&root,read,&mut budget).is_err());
    }
}
