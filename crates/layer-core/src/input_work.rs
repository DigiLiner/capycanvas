use std::sync::{Mutex,MutexGuard,Condvar,atomic::{AtomicBool,Ordering}};

pub const MAX_ACTIVE_DECODERS:usize=2;
pub const MAX_BROWSER_WORKERS:usize=8;
pub const MAX_BROWSER_REQUESTS:usize=32;
pub const MAX_BROWSER_PAYLOAD_BYTES:usize=256*1024*1024;
pub const BROWSER_RESULT_LIFETIME_MS:u32=180_000;

pub struct InputWork {active:Mutex<usize>,ready:Condvar,maximum:usize}
pub struct InputWorkGuard<'a>(&'a InputWork);
pub static DECODERS:InputWork=InputWork::new(MAX_ACTIVE_DECODERS);
impl InputWork {
    pub const fn new(maximum:usize)->Self {Self {active:Mutex::new(0),ready:Condvar::new(),maximum}}
    pub fn enter(&self,cancelled:&AtomicBool)->Result<InputWorkGuard<'_>,String> {
        let mut active=self.active.lock().map_err(|_|"Input decoder admission stopped")?;
        loop {
            if cancelled.load(Ordering::Acquire) {return Err("Input operation cancelled".into());}
            if *active<self.maximum {*active+=1;return Ok(InputWorkGuard(self));}
            active=self.wait(active)?;
        }
    }
    #[cfg(not(target_arch="wasm32"))]
    fn wait<'a>(&self,active:MutexGuard<'a,usize>)->Result<MutexGuard<'a,usize>,String> {
        Ok(self.ready.wait_timeout(active,std::time::Duration::from_millis(50)).map_err(|_|"Input decoder admission stopped")?.0)
    }
    #[cfg(target_arch="wasm32")]
    fn wait<'a>(&self,_active:MutexGuard<'a,usize>)->Result<MutexGuard<'a,usize>,String> {Err("Input decoder is busy".into())}
}
impl Drop for InputWorkGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut active)=self.0.active.lock() {*active-=1;self.0.ready.notify_one();}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc,mpsc};
    #[test]
    fn waiting_decoders_do_not_enter_until_the_previous_execution_stops() {
        let work=Arc::new(InputWork::new(1));let running=work.enter(&AtomicBool::new(false)).unwrap();
        let (entered,events)=mpsc::channel();let next=work.clone();
        let worker=std::thread::spawn(move||{let _guard=next.enter(&AtomicBool::new(false)).unwrap();entered.send(()).unwrap();});
        assert!(events.recv_timeout(std::time::Duration::from_millis(100)).is_err());
        drop(running);events.recv_timeout(std::time::Duration::from_secs(2)).unwrap();worker.join().unwrap();
        assert_eq!(*work.active.lock().unwrap(),0);
    }
    #[test]
    fn cancellation_releases_a_waiter_without_stealing_the_running_slot() {
        let work=Arc::new(InputWork::new(1));let cancelled=Arc::new(AtomicBool::new(false));
        let running=work.enter(&AtomicBool::new(false)).unwrap();let next=work.clone();let cancel=cancelled.clone();
        let worker=std::thread::spawn(move||next.enter(&cancel).map(|_|()).unwrap_err());
        cancelled.store(true,Ordering::Release);assert_eq!(worker.join().unwrap(),"Input operation cancelled");
        assert_eq!(*work.active.lock().unwrap(),1);drop(running);assert_eq!(*work.active.lock().unwrap(),0);
    }
}
