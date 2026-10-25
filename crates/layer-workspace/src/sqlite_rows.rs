use super::*;
use rusqlite::{Row,types::{Type,ValueRef}};

fn conversion(index:usize,error:StoreError)->rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index,Type::Text,Box::new(error))
}
pub(super) fn text(row:&Row<'_>,index:usize,limit:usize)->rusqlite::Result<String> {
    let ValueRef::Text(bytes)=row.get_ref(index)? else {return Err(conversion(index,StoreError::invalid("Invalid workspace text field.")));};
    if bytes.len()>limit {return Err(conversion(index,crate::input::limit()));}
    let source=std::str::from_utf8(bytes).map_err(|error|conversion(index,StoreError::invalid(error.to_string())))?;
    crate::input::copy(source,limit).map_err(|error|conversion(index,error))
}
pub(super) fn optional_text(row:&Row<'_>,index:usize,limit:usize)->rusqlite::Result<Option<String>> {
    if row.get_ref(index)?==ValueRef::Null {Ok(None)}
    else {text(row,index,limit).map(Some)}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn borrowed_fields_check_type_utf8_and_size_before_copy() {
        let db=Connection::open_in_memory().unwrap();
        for sql in ["SELECT NULL","SELECT x'6162'","SELECT CAST(x'ff' AS TEXT)","SELECT 'oversized'"] {
            let error=db.query_row(sql,[],|row|text(row,0,4)).unwrap_err();
            assert_eq!(StoreError::from(error).kind,ErrorKind::InvalidData);
        }
        assert_eq!(db.query_row("SELECT 'abcd'",[],|row|text(row,0,4)).unwrap(),"abcd");
        assert_eq!(db.query_row("SELECT NULL",[],|row|optional_text(row,0,4)).unwrap(),None);
    }
}
