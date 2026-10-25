use super::Error;
use std::{cell::RefCell,ops::{Deref,DerefMut},rc::Rc};

pub struct LocalLease<T>(Rc<RefCell<Option<Box<T>>>>);
impl<T> Clone for LocalLease<T> {fn clone(&self)->Self {Self(self.0.clone())}}
impl<T> LocalLease<T> {
    pub fn new(value:T)->Self {Self(Rc::new(RefCell::new(Some(Box::new(value)))))}
    pub fn borrow(&self)->Result<LocalBorrowed<T>,Error> {
        let value=self.0.try_borrow_mut().map_err(|_|Error::Busy)?.take().ok_or(Error::Busy)?;
        Ok(LocalBorrowed {value:Some(value),slot:self.0.clone()})
    }
}
pub struct LocalBorrowed<T> {value:Option<Box<T>>,slot:Rc<RefCell<Option<Box<T>>>>}
impl<T> Deref for LocalBorrowed<T> {type Target=T;fn deref(&self)->&T {self.value.as_deref().unwrap()}}
impl<T> DerefMut for LocalBorrowed<T> {fn deref_mut(&mut self)->&mut T {self.value.as_deref_mut().unwrap()}}
impl<T> Drop for LocalBorrowed<T> {
    fn drop(&mut self) {
        if !std::thread::panicking() && let Ok(mut slot)=self.slot.try_borrow_mut() {*slot=self.value.take();}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handles::{Handle,LocalHandles};
    use std::{cell::Cell,panic::{AssertUnwindSafe,catch_unwind}};

    struct Value(Rc<Cell<usize>>);
    impl Drop for Value {fn drop(&mut self) {self.0.set(self.0.get()+1);}}

    #[test]
    fn owner_lookup_returns_an_owned_non_send_guard_without_moving_state() {
        let registry=LocalHandles::default();let handle=registry.insert(LocalLease::new(Rc::new(Cell::new(17)))).unwrap();
        let lookup=||registry.with::<LocalLease<Rc<Cell<i32>>>,_>(handle,|lease|lease.clone()).unwrap();
        let lease=lookup();let address;
        {let value=lease.borrow().unwrap();address=&*value as *const Rc<Cell<i32>>;value.set(19);
            assert_eq!(lookup().borrow().err(),Some(Error::Busy));}
        {let value=lookup().borrow().unwrap();assert!(std::ptr::eq(address,&*value));assert_eq!(value.get(),19);}
        registry.take::<LocalLease<Rc<Cell<i32>>>>(handle).unwrap();
        assert_eq!(registry.with::<LocalLease<Rc<Cell<i32>>>,_>(handle,|_|()).err(),Some(Error::Invalid));
    }

    #[test]
    fn registry_release_keeps_active_local_value_alive_until_its_guard_drops() {
        let drops=Rc::new(Cell::new(0));let registry=LocalHandles::default();
        let handle=registry.insert(LocalLease::new(Value(drops.clone()))).unwrap();
        let lease=registry.with::<LocalLease<Value>,_>(handle,|lease|lease.clone()).unwrap();
        let active=lease.borrow().unwrap();drop(lease);
        drop(registry.take::<LocalLease<Value>>(handle).unwrap());drop(registry);
        assert_eq!(drops.get(),0);drop(active);assert_eq!(drops.get(),1);
    }

    #[test]
    fn a_local_panic_drops_partial_state_and_leaves_other_handles_available() {
        let drops=Rc::new(Cell::new(0));let lease=LocalLease::new(Value(drops.clone()));
        assert!(catch_unwind(AssertUnwindSafe(|| {let _active=lease.borrow().unwrap();panic!("Partial local state");})).is_err());
        assert_eq!(drops.get(),1);assert_eq!(lease.borrow().err(),Some(Error::Busy));
        let registry=LocalHandles::default();let other=registry.insert(LocalLease::new(31usize)).unwrap();
        assert_eq!(**registry.with::<LocalLease<usize>,_>(other,|lease|lease.clone()).unwrap().borrow().as_ref().unwrap(),31);
        assert!(Handle::from_raw(other.raw()).is_ok());
    }
}
