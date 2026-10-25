use super::*;
use rusqlite::{Row,types::{Type,ValueRef}};
use serde::de::DeserializeOwned;
use std::{cell::RefCell,time::{Duration,Instant}};

const MAX_CELLS:usize=1_000_000;
const MAX_SQL_STEPS:usize=10_000_000;
struct Budget {depth:usize,bytes:usize,cells:usize,nodes:usize,failed:bool,steps:usize,deadline:Instant}
thread_local! {static BUDGET:RefCell<Option<Budget>>=Default::default();}
pub(crate) struct Guard(std::marker::PhantomData<std::rc::Rc<()>>);
impl Guard {
    pub fn new()->Self {
        BUDGET.with(|slot| {
            let mut state=slot.borrow_mut();
            if let Some(state)=state.as_mut() {state.depth+=1;}
            else {*state=Some(Budget {depth:1,bytes:MAX_PACKAGE_BYTES,cells:MAX_CELLS,nodes:crate::input::MAX_INPUT_NODES,
                failed:false,steps:MAX_SQL_STEPS,deadline:Instant::now()+Duration::from_secs(15)});}
        });Self(std::marker::PhantomData)
    }
}
impl Drop for Guard {
    fn drop(&mut self) {BUDGET.with(|slot| {let mut state=slot.borrow_mut();
        if let Some(budget)=state.as_mut() {budget.depth-=1;if budget.depth==0 {*state=None;}}
    });}
}
pub(crate) fn install(connection:&Connection)->rusqlite::Result<()> {
    connection.progress_handler(1000,Some(||BUDGET.with(|slot| {
        let Ok(mut state)=slot.try_borrow_mut() else {return true;};
        let Some(budget)=state.as_mut() else {return false;};
        if budget.failed || budget.steps<1000 || Instant::now()>=budget.deadline {return true;}
        budget.steps-=1000;false
    })))
}
fn charge(bytes:usize)->Result<()> {
    BUDGET.with(|slot| {
        let mut state=slot.borrow_mut();
        if let Some(budget)=state.as_mut() {
            if budget.failed || bytes>budget.bytes || budget.cells==0 {budget.failed=true;return Err(crate::input::limit());}
            budget.bytes-=bytes;budget.cells-=1;
            if Instant::now()>=budget.deadline {return Err(crate::input::limit());}
        }Ok(())
    })
}
fn conversion(index:usize,error:StoreError)->rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index,Type::Text,Box::new(error))
}
pub(super) fn text(row:&Row<'_>,index:usize,limit:usize)->rusqlite::Result<String> {
    let ValueRef::Text(bytes)=row.get_ref(index)? else {return Err(conversion(index,StoreError::invalid("Invalid workspace text field.")));};
    if bytes.len()>limit {return Err(conversion(index,crate::input::limit()));}
    charge(bytes.len()).map_err(|error|conversion(index,error))?;
    let source=std::str::from_utf8(bytes).map_err(|error|conversion(index,StoreError::invalid(error.to_string())))?;
    crate::input::copy(source,limit).map_err(|error|conversion(index,error))
}
pub(super) fn optional_text(row:&Row<'_>,index:usize,limit:usize)->rusqlite::Result<Option<String>> {
    if row.get_ref(index)?==ValueRef::Null {charge(0).map_err(|error|conversion(index,error))?;Ok(None)}
    else {text(row,index,limit).map(Some)}
}
pub(crate) fn work()->Result<()> {charge(0)}
pub(super) fn json<T:DeserializeOwned>(text:&str)->Result<T> {crate::input::json(text)}
pub(crate) fn remaining_nodes()->usize {BUDGET.with(|slot|slot.borrow().as_ref().map_or(crate::input::MAX_INPUT_NODES,|budget|budget.nodes))}
pub(crate) fn consumed_nodes(count:usize) {BUDGET.with(|slot| {if let Some(budget)=slot.borrow_mut().as_mut() {budget.nodes-=count;}})}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrowed_fields_check_type_utf8_size_and_shared_budget_before_copy() {
        let db=Connection::open_in_memory().unwrap();let _guard=Guard::new();
        for sql in ["SELECT NULL","SELECT x'6162'","SELECT CAST(x'ff' AS TEXT)","SELECT 'oversized'"] {
            let error=db.query_row(sql,[],|row|text(row,0,4)).unwrap_err();
            assert_eq!(StoreError::from(error).kind,ErrorKind::InvalidData);
        }
        BUDGET.with(|slot|slot.borrow_mut().as_mut().unwrap().bytes=5);
        assert_eq!(db.query_row("SELECT 'abcd'",[],|row|text(row,0,4)).unwrap(),"abcd");
        let nested=Guard::new();
        assert!(db.query_row("SELECT 'ef'",[],|row|text(row,0,4)).is_err());drop(nested);
        assert!(db.query_row("SELECT ''",[],|row|text(row,0,4)).is_err());
    }

    #[test]
    fn sql_work_interrupts_before_a_result_rolls_back_and_resets() {
        let mut db=Connection::open_in_memory().unwrap();install(&db).unwrap();
        db.execute_batch("CREATE TABLE value(n INTEGER);INSERT INTO value VALUES(7)").unwrap();
        {
            let _guard=Guard::new();let tx=db.transaction().unwrap();tx.execute("UPDATE value SET n=9",[]).unwrap();
            BUDGET.with(|slot|slot.borrow_mut().as_mut().unwrap().steps=0);
            let error=tx.query_row("WITH RECURSIVE counter(n) AS (VALUES(0) UNION ALL SELECT n+1 FROM counter WHERE n<1000000) SELECT sum(n) FROM counter",[],|row|row.get::<_,i64>(0)).unwrap_err();
            assert_eq!(StoreError::from(error).kind,ErrorKind::InvalidData);
        }
        let _guard=Guard::new();
        assert_eq!(db.query_row("SELECT n FROM value",[],|row|row.get::<_,i64>(0)).unwrap(),7);
        assert_eq!(json::<Vec<u64>>("[1,2,3]").unwrap(),[1,2,3]);
    }

    #[test]
    fn stored_json_nodes_share_the_whole_request_allowance() {
        let _guard=Guard::new();BUDGET.with(|slot|slot.borrow_mut().as_mut().unwrap().nodes=4);
        assert_eq!(json::<Vec<u64>>("[1,2]").unwrap(),[1,2]);
        assert!(json::<Vec<u64>>("[3]").is_err());
    }
}
