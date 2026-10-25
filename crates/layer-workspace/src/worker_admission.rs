use crate::*;
use std::sync::{Arc,Mutex,OnceLock};

const MAX_JOBS:usize=32;
const MAX_BYTES:usize=256*1024*1024;

#[derive(Default)]
struct Usage {jobs:usize,bytes:usize}
pub(super) struct Admission {jobs:usize,bytes:usize,usage:Mutex<Usage>}
pub(super) struct Permit {admission:Arc<Admission>,bytes:usize}
fn busy()->StoreError {StoreError::new(ErrorKind::Unavailable,"Workspace storage is busy. Try again after the current operation finishes.")}
impl Admission {
    pub fn shared()->Arc<Self> {
        static ADMISSION:OnceLock<Arc<Admission>>=OnceLock::new();
        ADMISSION.get_or_init(||Arc::new(Self::new(MAX_JOBS,MAX_BYTES))).clone()
    }
    pub(super) fn new(jobs:usize,bytes:usize)->Self {Self {jobs,bytes,usage:Mutex::new(Usage::default())}}
    pub fn acquire(self:&Arc<Self>,bytes:usize)->Result<Arc<Permit>,StoreError> {
        let mut usage=self.usage.lock().map_err(|_|busy())?;
        let next=usage.bytes.checked_add(bytes).filter(|total|*total<=self.bytes).ok_or_else(busy)?;
        if usage.jobs>=self.jobs {return Err(busy());}
        usage.jobs+=1;usage.bytes=next;
        Ok(Arc::new(Permit {admission:self.clone(),bytes}))
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        if let Ok(mut usage)=self.admission.usage.lock() {usage.jobs-=1;usage.bytes-=self.bytes;}
    }
}
struct Cost(usize);
impl Cost {
    fn add(&mut self,bytes:usize)->Result<(),StoreError> {
        self.0=self.0.checked_add(bytes).filter(|total|*total<=MAX_BYTES).ok_or_else(busy)?;Ok(())
    }
    fn string(&mut self,value:&String)->Result<(),StoreError> {self.add(value.capacity())}
    fn strings(&mut self,values:&Vec<String>)->Result<(),StoreError> {
        self.add(values.capacity().checked_mul(std::mem::size_of::<String>()).ok_or_else(busy)?)?;
        for value in values {self.string(value)?;}Ok(())
    }
    fn owner(&mut self,owner:&Owner)->Result<(),StoreError> {self.string(&owner.id)?;self.string(&owner.epoch)}
}
pub(super) fn request_bytes(request:&StoreRequest)->Result<usize,StoreError> {
    let mut cost=Cost(std::mem::size_of::<StoreRequest>());
    match request {
        StoreRequest::UpdateSwitcher {expected,ids}|StoreRequest::UpdateWorkspaceOrder {expected,ids}=>{
            if let Some(expected)=expected {cost.strings(expected)?;}cost.strings(ids)?;
        },
        StoreRequest::Load {id}=>cost.string(id)?,
        StoreRequest::Claim {id,owner}=>{cost.string(id)?;cost.owner(owner)?;},
        StoreRequest::Renew {id,owner,fence}|StoreRequest::Release {id,owner,fence}=>{cost.string(id)?;cost.owner(owner)?;cost.string(fence)?;},
        StoreRequest::Commit {batch}=>{
            cost.string(&batch.operation_id)?;cost.owner(&batch.owner)?;
            if let Some(value)=&batch.editing_json {cost.string(value)?;}
            cost.add(batch.writes.capacity().checked_mul(std::mem::size_of::<crate::protocol::PreparedWrite>()).ok_or_else(busy)?)?;
            for write in &batch.writes {
                cost.string(&write.id)?;
                if let Some(metadata)=&write.metadata {cost.string(&metadata.name)?;cost.string(&metadata.description)?;}
                for value in [&write.metadata_json,&write.content_json,&write.working_json].into_iter().flatten() {cost.string(value)?;}
            }
            cost.add(batch.components.len().checked_mul(256).ok_or_else(busy)?)?;
            for (key,value) in &batch.components {cost.string(key)?;cost.string(value)?;}
            cost.add(batch.bindings.capacity().checked_mul(std::mem::size_of::<(String,Option<String>)>()).ok_or_else(busy)?)?;
            for (key,value) in &batch.bindings {cost.string(key)?;if let Some(value)=value {cost.string(value)?;}}
            cost.strings(&batch.abandon_operations)?;cost.strings(&batch.pin_workspaces)?;
        },
        StoreRequest::Acknowledge {operation_id}|StoreRequest::Receipt {operation_id}=>cost.string(operation_id)?,
        StoreRequest::Binding {key}=>cost.string(key)?,
        StoreRequest::Maintenance {owner:Some(owner),..}=>cost.owner(owner)?,
        StoreRequest::Editing|StoreRequest::List|StoreRequest::Switcher|StoreRequest::WorkspaceOrder|StoreRequest::Pending|StoreRequest::Maintenance {owner:None,..}|StoreRequest::Reopen|StoreRequest::Reset=>(),
    }
    Ok(cost.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_permits_keep_capacity_until_workers_and_replies_both_release_it() {
        let admission=Arc::new(Admission::new(2,7));
        let first=admission.acquire(3).unwrap();let worker=first.clone();
        let second=admission.acquire(4).unwrap();
        assert!(admission.acquire(0).is_err());
        drop(first);assert!(admission.acquire(1).is_err());
        drop(worker);assert!(admission.acquire(4).is_err());
        let replacement=admission.acquire(3).unwrap();drop(second);drop(replacement);
        assert!(admission.acquire(7).is_ok());
        assert!(admission.acquire(usize::MAX).is_err());
    }
    #[test]
    fn request_cost_counts_capacity_and_duplicate_owned_payloads() {
        let mut id=String::with_capacity(4096);id.push('a');
        assert_eq!(request_bytes(&StoreRequest::Load {id}).unwrap(),std::mem::size_of::<StoreRequest>()+4096);
        let batch=CommitBatch::prepare(Owner::fresh(),Vec::new()).unwrap();
        let mut request=StoreRequest::Commit {batch};
        let before=request_bytes(&request).unwrap();
        if let StoreRequest::Commit {batch}=&mut request {
            batch.components.insert(String::with_capacity(128),String::with_capacity(512));
            batch.bindings.push((String::with_capacity(256),Some(String::with_capacity(1024))));
        }
        assert!(request_bytes(&request).unwrap()>=before+128+512+256+1024+256);
    }
}
