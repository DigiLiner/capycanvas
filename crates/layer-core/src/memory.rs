use std::{cell::Cell,collections::HashMap,sync::{Arc,Mutex,OnceLock,Weak}};

thread_local! {static INPUT:Cell<bool>=Default::default();}
pub fn with_input<T>(work:impl FnOnce()->T)->T {
    struct Restore(bool);
    impl Drop for Restore {fn drop(&mut self) {INPUT.with(|input|input.set(self.0));}}
    let _restore=Restore(INPUT.with(|input|input.replace(true)));work()
}

const OWNER_CHARGE:usize=256;
const MAX_OWNERS:usize=262_144;
static BUDGET:OnceLock<Budget>=OnceLock::new();
mod sealed {pub trait Payload {} impl Payload for [u8] {} impl Payload for [u32] {} impl Payload for str {}}
pub trait Payload:sealed::Payload+Send+Sync {}
impl Payload for [u8] {} impl Payload for [u32] {} impl Payload for str {}
trait Owner:Send+Sync {fn alive(&self)->bool;}
struct Allocation<T:?Sized>(Weak<T>);
impl<T:?Sized+Send+Sync> Owner for Allocation<T> {fn alive(&self)->bool {self.0.strong_count()!=0}}
struct Entry {owner:Box<dyn Owner>,bytes:usize}
#[derive(Default)]
struct State {used:usize,owners:HashMap<usize,Entry>}
pub struct Budget {maximum:usize,state:Mutex<State>,released:std::sync::atomic::AtomicU64}
pub struct Reservation<'a> {budget:&'a Budget,bytes:usize,limit:usize}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct MemoryError;
impl std::fmt::Display for MemoryError {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {f.write_str("Retained image data exceeds the shared memory budget")}}
impl std::error::Error for MemoryError {}
impl From<MemoryError> for String {fn from(error:MemoryError)->Self {error.to_string()}}
fn exhausted()->MemoryError {MemoryError}
impl Budget {
    pub fn new(maximum:usize)->Self {Self {maximum,state:Mutex::new(State::default()),released:std::sync::atomic::AtomicU64::new(0)}}
    pub fn generation(&self)->u64 {self.released.load(std::sync::atomic::Ordering::Acquire)}
    fn reclaim(&self,state:&mut State) {
        let before=state.used;
        state.owners.retain(|_,entry| {
            if entry.owner.alive() {true}else{state.used-=entry.bytes;false}
        });
        if state.used<before {self.released.fetch_add(1,std::sync::atomic::Ordering::Release);}
    }
    pub fn collect(&self)->Result<(),MemoryError> {
        if BUDGET.get().is_some_and(|budget|std::ptr::eq(self,budget)) {crate::raster::DecodedTileCache::collect_shared().map_err(|_|MemoryError)?;}
        let mut state=self.state.lock().map_err(|_|exhausted())?;
        self.reclaim(&mut state);Ok(())
    }
    fn state_for(&self,extra:usize,limit:usize)->Result<std::sync::MutexGuard<'_,State>,MemoryError> {
        let mut state=self.state.lock().map_err(|_|exhausted())?;
        if extra>limit.saturating_sub(state.used) {
            self.reclaim(&mut state);
            if extra>limit.saturating_sub(state.used) && BUDGET.get().is_some_and(|budget|std::ptr::eq(self,budget)) {
                drop(state);crate::raster::DecodedTileCache::evict_shared();
                state=self.state.lock().map_err(|_|exhausted())?;self.reclaim(&mut state);
            }
        }
        Ok(state)
    }
    pub fn reserve(&self,bytes:usize)->Result<Reservation<'_>,MemoryError> {
        let limit=if INPUT.with(Cell::get) {self.maximum-self.maximum/4}else {self.maximum};
        let mut state=self.state_for(bytes,limit)?;
        state.used=state.used.checked_add(bytes).filter(|used|bytes==0 || *used<=limit).ok_or_else(exhausted)?;
        Ok(Reservation {budget:self,bytes,limit})
    }
    pub fn track<T:?Sized+Payload+'static>(&self,value:&Arc<T>)->Result<(),MemoryError> {
        self.reserve(0)?.retain(value.clone()).map(|_|())
    }
    pub fn maximum(&self)->usize {self.maximum}
    pub fn reserve_payload(&self,bytes:usize)->Result<Reservation<'_>,MemoryError> {self.reserve(bytes.checked_add(OWNER_CHARGE).ok_or_else(exhausted)?)}
}
impl Reservation<'_> {
    pub fn resize(&mut self,bytes:usize)->Result<(),MemoryError> {
        let mut state=self.budget.state_for(bytes.saturating_sub(self.bytes),self.limit)?;
        let next=state.used.checked_sub(self.bytes).and_then(|used|used.checked_add(bytes)).filter(|used|bytes<=self.bytes || *used<=self.limit).ok_or_else(exhausted)?;
        if bytes<self.bytes {self.budget.released.fetch_add(1,std::sync::atomic::Ordering::Release);}
        state.used=next;self.bytes=bytes;Ok(())
    }
    pub fn retain<T:?Sized+Payload+'static>(mut self,value:Arc<T>)->Result<Arc<T>,MemoryError> {
        let result=(|| {
            let key=Arc::as_ptr(&value) as *const () as usize;
            let bytes=std::mem::size_of_val(value.as_ref()).checked_add(OWNER_CHARGE).ok_or_else(exhausted)?;
            let mut state=self.budget.state.lock().map_err(|_|exhausted())?;
            if state.owners.contains_key(&key) {return Ok(());}
            drop(state);state=self.budget.state_for(bytes.saturating_sub(self.bytes),self.limit)?;
            if state.owners.contains_key(&key) {return Ok(());}
            if state.owners.len()>=MAX_OWNERS || bytes>self.bytes && bytes-self.bytes>self.limit.saturating_sub(state.used) {self.budget.reclaim(&mut state);}
            if state.owners.len()>=MAX_OWNERS {return Err(exhausted());}
            let next=state.used.checked_sub(self.bytes).and_then(|used|used.checked_add(bytes)).filter(|used|bytes<=self.bytes || *used<=self.limit).ok_or_else(exhausted)?;
            state.owners.try_reserve(1).map_err(|_|exhausted())?;
            state.owners.insert(key,Entry {owner:Box::new(Allocation(Arc::downgrade(&value))),bytes});
            if bytes<self.bytes {self.budget.released.fetch_add(1,std::sync::atomic::Ordering::Release);}
            state.used=next;self.bytes=0;Ok(())
        })();
        match result {Ok(())=>Ok(value),Err(error)=>{drop(value);Err(error)}}
    }
}
impl Drop for Reservation<'_> {
    fn drop(&mut self) {if let Ok(mut state)=self.budget.state.lock() {state.used-=self.bytes;if self.bytes!=0 {self.budget.released.fetch_add(1,std::sync::atomic::Ordering::Release);}}}
}
pub fn images()->&'static Budget {
    BUDGET.get_or_init(||Budget::new(capacity()))
}
pub fn generation()->u64 {BUDGET.get().map_or(0,Budget::generation)}
pub fn notify_capacity_released() {if let Some(budget)=BUDGET.get() {budget.released.fetch_add(1,std::sync::atomic::Ordering::Release);}}
#[derive(Clone,Debug,Default)]
pub struct OwnerRetirement;
impl Drop for OwnerRetirement {fn drop(&mut self) {notify_capacity_released();}}
pub struct DecodedBytes {bytes:Vec<u8>,_memory:Reservation<'static>}
impl DecodedBytes {
    pub fn zeroed(length:usize)->Result<Self,MemoryError> {
        let memory=images().reserve(length)?;let mut bytes=Vec::new();
        bytes.try_reserve_exact(length).map_err(|_|exhausted())?;bytes.resize(length,0);
        Self::from_reserved(bytes,memory)
    }
    pub fn copy_from(bytes:&[u8])->Result<Self,MemoryError> {
        let mut result=Self::zeroed(bytes.len())?;result.copy_from_slice(bytes);Ok(result)
    }
    pub(crate) fn from_reserved(bytes:Vec<u8>,mut memory:Reservation<'static>)->Result<Self,MemoryError> {
        memory.resize(bytes.capacity())?;Ok(Self {bytes,_memory:memory})
    }
    pub fn as_slice(&self)->&[u8] {&self.bytes}
    pub fn capacity(&self)->usize {self.bytes.capacity()}
    pub(crate) fn extend_from_slice(&mut self,bytes:&[u8])->Result<(),MemoryError> {
        extend_bytes(&mut self.bytes,&mut self._memory,bytes)
    }
    pub fn into_shared(self)->Result<Arc<[u8]>,MemoryError> {
        let Self {bytes,_memory:mut memory}=self;
        memory.resize(bytes.capacity().checked_add(bytes.len()).and_then(|bytes|bytes.checked_add(OWNER_CHARGE)).ok_or_else(exhausted)?)?;
        memory.retain(Arc::<[u8]>::from(bytes))
    }
}
fn extend_bytes(bytes:&mut Vec<u8>,memory:&mut Reservation<'_>,additional:&[u8])->Result<(),MemoryError> {
    let length=bytes.len().checked_add(additional.len()).ok_or_else(exhausted)?;
    if length>bytes.capacity() {
        let previous=bytes.capacity();
        let capacity=previous.saturating_mul(2).max(64).max(length).min(isize::MAX as usize);
        memory.resize(previous.checked_add(capacity).ok_or_else(exhausted)?)?;
        if bytes.try_reserve_exact(capacity-bytes.len()).is_err() {memory.resize(previous)?;return Err(exhausted());}
        memory.resize(bytes.capacity())?;
    }
    bytes.extend_from_slice(additional);Ok(())
}
impl std::ops::Deref for DecodedBytes {type Target=[u8];fn deref(&self)->&[u8] {&self.bytes}}
impl std::ops::DerefMut for DecodedBytes {fn deref_mut(&mut self)->&mut[u8] {&mut self.bytes}}
impl AsRef<[u8]> for DecodedBytes {fn as_ref(&self)->&[u8] {&self.bytes}}
impl std::fmt::Debug for DecodedBytes {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {self.bytes.fmt(f)}}
impl PartialEq for DecodedBytes {fn eq(&self,other:&Self)->bool {self.bytes==other.bytes}}
impl Eq for DecodedBytes {}
impl PartialEq<Vec<u8>> for DecodedBytes {fn eq(&self,other:&Vec<u8>)->bool {&self.bytes==other}}
#[cfg(target_arch="wasm32")]
fn capacity()->usize {2*1024*1024*1024-1}
#[cfg(not(target_arch="wasm32"))]
fn capacity()->usize {((available_memory().unwrap_or(512*1024*1024)/2).min(4*1024*1024*1024).min(isize::MAX as u64)) as usize}

#[cfg(target_arch = "wasm32")]
pub fn available_memory() -> Option<u64> {
    // Device RAM and JS heap limits are not available WASM process memory.
    // A host with a trustworthy budget supplies it per operation instead.
    None
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios", not(target_env = "sim")))]
pub fn available_memory() -> Option<u64> {
    unsafe extern "C" {
        fn os_proc_available_memory() -> usize;
    }
    // iOS 13+: remaining app allowance, including its memory-pressure limit.
    Some(unsafe { os_proc_available_memory() } as u64)
}

#[cfg(all(not(target_arch = "wasm32"), any(not(target_os = "ios"), target_env = "sim")))]
pub fn available_memory() -> Option<u64> {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    if system.total_memory() == 0 {
        return None;
    }
    // sysinfo 0.37's Apple available-memory calculation subtracts compressed
    // pages from free/inactive pages, even though they are already excluded.
    // Use its total/used readings to count compression once. Do not invent a
    // minimum: true exhaustion must still produce a zero admission budget.
    #[cfg(any(target_os = "macos", all(target_os = "ios", target_env = "sim")))]
    let headroom = system.total_memory().saturating_sub(system.used_memory());
    #[cfg(not(any(target_os = "macos", all(target_os = "ios", target_env = "sim"))))]
    let headroom = system.available_memory();
    let available = process_allowance(
        headroom,
        system.total_memory(),
        system
            .cgroup_limits()
            .map(|c| (c.total_memory, c.free_memory)),
    );
    // sysinfo reports an unlimited root cgroup as physical total minus current
    // usage, including reclaimable cache. That is not a process limit; applying
    // it would incorrectly replace Linux/Android MemAvailable with free pages.
    Some(available)
}

#[cfg(all(not(target_arch = "wasm32"), any(not(target_os = "ios"), target_env = "sim")))]
fn process_allowance(available: u64, total: u64, cgroup: Option<(u64, u64)>) -> u64 {
    match cgroup {
        Some((limit, free)) if limit < total => available.min(free),
        _ => available,
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(all(not(target_arch = "wasm32"), any(not(target_os = "ios"), target_env = "sim")))]
    fn unlimited_cgroup_does_not_discard_reclaimable_headroom() {
        assert_eq!(process_allowance(3000, 12000, Some((12000, 1800))), 3000);
        assert_eq!(process_allowance(3000, 12000, Some((4000, 1800))), 1800);
        assert_eq!(process_allowance(3000, 12000, Some((4000, 0))), 0);
        assert_eq!(process_allowance(0, 12000, None), 0);
    }
    #[test]
    fn raw_aliases_hold_their_charge_after_graph_owners_disappear() {
        let budget=Budget::new(512);let bytes:Arc<[u8]>=vec![7;128].into();
        budget.reserve(512).unwrap().retain(bytes.clone()).unwrap();
        let alias=bytes.clone();drop(bytes);budget.collect().unwrap();
        assert!(budget.reserve(129).is_err());drop(alias);
        assert!(budget.reserve(512).is_ok());
    }
    #[test]
    fn duplicate_allocations_share_one_charge_but_equal_bytes_do_not() {
        let budget=Budget::new(512);let bytes:Arc<[u8]>=vec![7;128].into();
        budget.track(&bytes).unwrap();budget.track(&bytes.clone()).unwrap();
        let other:Arc<[u8]>=vec![7;128].into();assert!(budget.track(&other).is_err());
        assert_eq!(budget.state.lock().unwrap().used,384);
    }
    #[test]
    fn failed_growth_and_retention_preserve_other_reservations() {
        let budget=Budget::new(512);let mut scratch=budget.reserve(300).unwrap();let other=budget.reserve(200).unwrap();
        assert!(scratch.resize(400).is_err());assert_eq!(scratch.bytes,300);
        let bytes:Arc<[u8]>=vec![0;128].into();assert!(scratch.retain(bytes.clone()).is_err());
        assert_eq!(budget.state.lock().unwrap().used,200);drop(other);assert!(budget.reserve(512).is_ok());
    }
    #[test]
    fn empty_payloads_and_overflow_cannot_bypass_the_allowance() {
        let budget=Budget::new(512);let a:Arc<[u8]>=Arc::from([]);let b:Arc<[u8]>=Arc::from([]);let c:Arc<[u8]>=Arc::from([]);
        budget.track(&a).unwrap();budget.track(&b).unwrap();assert!(budget.track(&c).is_err());
        assert!(budget.reserve(usize::MAX).is_err());
        drop(a);drop(b);budget.collect().unwrap();assert_eq!(budget.state.lock().unwrap().used,0);
    }
    #[test]
    fn input_work_keeps_capacity_available_for_live_canvas_buffers() {
        let budget=Budget::new(512);
        let input=with_input(||budget.reserve(384)).unwrap();
        assert!(with_input(||budget.reserve(1)).is_err());
        let canvas=budget.reserve(128).unwrap();
        assert!(budget.reserve(1).is_err());drop(input);drop(canvas);
        assert!(budget.reserve(512).is_ok());
        let _=std::panic::catch_unwind(||with_input(||panic!("cancelled input")));
        assert!(budget.reserve(512).is_ok());
    }
    #[test]
    fn concurrent_reservations_cannot_overbook_the_pool() {
        use std::sync::{Barrier,mpsc};
        let budget=Arc::new(Budget::new(512));let release=Arc::new(Barrier::new(3));let (send,receive)=mpsc::channel();
        std::thread::scope(|scope| {
            for _ in 0..2 {
                let budget=&budget;let release=&release;let send=&send;
                scope.spawn(move||{let memory=budget.reserve(300);send.send(memory.is_ok()).unwrap();release.wait();drop(memory);});
            }
            let results=[receive.recv().unwrap(),receive.recv().unwrap()];
            assert_eq!(results.into_iter().filter(|result|*result).count(),1);release.wait();
        });
        assert!(budget.reserve(512).is_ok());
    }
    #[test]
    fn retry_generation_changes_only_when_charged_capacity_is_released() {
        let budget=Budget::new(512);let initial=budget.generation();
        let mut memory=budget.reserve(512).unwrap();assert_eq!(budget.generation(),initial);
        assert!(budget.reserve(1).is_err());assert_eq!(budget.generation(),initial);
        drop(budget.reserve(0).unwrap());assert_eq!(budget.generation(),initial);
        memory.resize(400).unwrap();let released=budget.generation();assert_ne!(released,initial);
        let bytes:Arc<[u8]>=vec![1;128].into();let bytes=memory.retain(bytes).unwrap();
        let retained=budget.generation();budget.track(&bytes).unwrap();assert_eq!(budget.generation(),retained);
        drop(bytes);budget.collect().unwrap();assert_ne!(budget.generation(),retained);
    }
    #[test]
    fn decoded_byte_growth_reserves_overlap_and_preserves_failed_appends() {
        let budget=Budget::new(192);let mut bytes=Vec::with_capacity(64);bytes.extend_from_slice(&[7;64]);
        let mut memory=budget.reserve(bytes.capacity()).unwrap();let other=budget.reserve(1).unwrap();
        assert_eq!(extend_bytes(&mut bytes,&mut memory,&[8]),Err(MemoryError));
        assert_eq!(bytes,&[7;64]);assert_eq!(bytes.capacity(),64);assert_eq!(memory.bytes,64);
        assert_eq!(budget.state.lock().unwrap().used,65);
        drop(other);extend_bytes(&mut bytes,&mut memory,&[8]).unwrap();
        assert_eq!(bytes.len(),65);assert_eq!(bytes[64],8);assert_eq!(bytes.capacity(),128);
        assert_eq!(memory.bytes,128);assert_eq!(budget.state.lock().unwrap().used,128);
        let remaining=budget.reserve(64).unwrap();let generation=budget.generation();
        extend_bytes(&mut bytes,&mut memory,&[9;63]).unwrap();
        extend_bytes(&mut bytes,&mut memory,&[]).unwrap();
        assert_eq!(bytes.len(),128);assert_eq!(&bytes[65..],&[9;63]);
        assert_eq!(memory.bytes,128);assert_eq!(budget.generation(),generation);
        assert_eq!(extend_bytes(&mut bytes,&mut memory,&[10]),Err(MemoryError));
        assert_eq!(bytes.len(),128);assert_eq!(memory.bytes,128);drop(remaining);drop(memory);
        assert!(budget.reserve(192).is_ok());
    }
}
