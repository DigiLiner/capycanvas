use crate::*;
use std::{
    collections::{BTreeMap,BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, Weak, mpsc},
};
#[path="worker_admission.rs"]
mod admission;

type Result<T> = std::result::Result<T, StoreError>;
enum WorkerRequest {
    Store(StoreRequest),
    Backup(PathBuf),
    RetireOwner(Owner),
}
type Message = (WorkerRequest, async_channel::Sender<Result<StoreResponse>>,Option<Arc<admission::Permit>>);
pub struct StoreReply {
    receiver:async_channel::Receiver<Result<StoreResponse>>,
    permit:Option<Arc<admission::Permit>>,
}

struct Worker {
    sender: Option<mpsc::Sender<Message>>,
    thread: Option<std::thread::JoinHandle<()>>,
    admission:Arc<admission::Admission>,
    retiring:Arc<Mutex<BTreeSet<(String,String)>>>,
}
impl Drop for Worker {
    fn drop(&mut self) {
        // The last native window drains already accepted requests and joins
        // SQLite teardown before its Rust library/process can be unloaded.
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
#[derive(Clone)]
pub struct StoreWorker(Arc<Worker>);
impl StoreWorker {
    /// Windows using one private directory share the same native I/O worker.
    /// Other processes coordinate through kernel locks and SQLite owner fences.
    pub fn shared(directory: &Path) -> Result<Self> {
        static WORKERS: OnceLock<Mutex<BTreeMap<PathBuf, Weak<Worker>>>> = OnceLock::new();
        let path = if directory.is_absolute() {
            directory.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|e| StoreError::new(ErrorKind::Unavailable, e.to_string()))?
                .join(directory)
        };
        let mut workers = WORKERS.get_or_init(Default::default).lock().map_err(|_| {
            StoreError::new(
                ErrorKind::Unavailable,
                "Workspace storage registry stopped.",
            )
        })?;
        workers.retain(|_, worker| worker.strong_count() > 0);
        if let Some(worker) = workers.get(&path).and_then(Weak::upgrade) {
            return Ok(Self(worker));
        }
        let (sender, receiver) = mpsc::channel::<Message>();
        let database = path.join("workspaces.sqlite3");
        let retiring=Arc::new(Mutex::new(BTreeSet::new()));
        let cleanup=retiring.clone();
        let thread = std::thread::Builder::new()
            .name("workspace-storage".into())
            .spawn(move || {
                let mut store = SqliteStore::open(&database);
                while let Ok((request, reply, _permit)) = receiver.recv() {
                    let request = match request {
                        WorkerRequest::RetireOwner(owner) => {
                            if let Ok(store) = &mut store {
                                store.retire_owner(&owner);
                            }
                            if let Ok(mut cleanup)=cleanup.lock() {cleanup.remove(&(owner.id,owner.epoch));}
                            continue;
                        }
                        WorkerRequest::Backup(destination) => {
                            let _ = reply.try_send(
                                backup_database(&database, &destination)
                                    .map(|()| StoreResponse::Done),
                            );
                            continue;
                        }
                        WorkerRequest::Store(request) => request,
                    };
                    if matches!(request, StoreRequest::Reopen | StoreRequest::Reset)
                        && store.is_err()
                    {
                        store = SqliteStore::open(&database);
                    }
                    let result = match &mut store {
                        Ok(store) => store.handle(request),
                        Err(error) => Err(error.clone()),
                    };
                    // A dropped receiver leaves the persisted receipt/pending payload
                    // available for retry. It must never imply that a write failed.
                    let _ = reply.try_send(result);
                }
            })
            .map_err(|e| StoreError::new(ErrorKind::Unavailable, e.to_string()))?;
        let worker = Arc::new(Worker {
            sender: Some(sender),
            thread: Some(thread),
            admission:admission::Admission::shared(),
            retiring,
        });
        workers.insert(path, Arc::downgrade(&worker));
        Ok(Self(worker))
    }
    pub fn request(&self, request: StoreRequest) -> StoreReply {
        let cost=admission::request_bytes(&request);
        self.enqueue(WorkerRequest::Store(request),cost)
    }
    fn enqueue(&self,request:WorkerRequest,cost:Result<usize>)->StoreReply {
        let (sender,receiver)=async_channel::bounded(1);
        let permit=cost.and_then(|bytes|self.0.admission.acquire(bytes));
        match permit {
            Ok(permit)=>{
                let _=self.0.sender.as_ref().unwrap().send((request,sender,Some(permit.clone())));
                StoreReply {receiver,permit:Some(permit)}
            },
            Err(error)=>{let _=sender.try_send(Err(error));StoreReply {receiver,permit:None}},
        }
    }
    /// Native recovery also works for databases with an unsupported model
    /// schema: SQLite's backup API reads the coherent snapshot including WAL.
    pub fn backup_database(&self, destination: &Path) -> StoreReply {
        let bytes=destination.as_os_str().as_encoded_bytes().len();
        if bytes>64*1024 {return self.enqueue(WorkerRequest::Backup(PathBuf::new()),Err(StoreError::invalid("Workspace backup path exceeds supported limits.")));}
        self.enqueue(WorkerRequest::Backup(destination.to_path_buf()),Ok(bytes*2+std::mem::size_of::<WorkerRequest>()))
    }
}

/// Native file transport must not replace the live database or its sidecars.
/// Call on an I/O worker. Canonical parents also catch not-yet-created WAL/SHM
/// files and folder aliases; in-memory package export works if storage vanished.
pub fn validate_database_export_destination(source: &Path, destination: &Path) -> Result<()> {
    fn resolved(path: &Path) -> PathBuf {
        std::fs::canonicalize(path)
            .or_else(|_| {
                std::fs::canonicalize(path.parent().unwrap_or(Path::new(".")))
                    .map(|parent| parent.join(path.file_name().unwrap_or_default()))
            })
            .unwrap_or_else(|_| std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()))
    }
    let destination = resolved(destination);
    let lock_directory = resolved(&crate::sqlite::ownership::lock_directory(&resolved(source)));
    #[cfg(windows)]
    let in_locks = destination
        .to_string_lossy()
        .to_lowercase()
        .starts_with(&(lock_directory.to_string_lossy().to_lowercase() + "\\"))
        || destination
            .to_string_lossy()
            .eq_ignore_ascii_case(&lock_directory.to_string_lossy());
    #[cfg(not(windows))]
    let in_locks = destination.starts_with(&lock_directory);
    if in_locks {
        return Err(StoreError::invalid(
            "Choose an export file outside the live workspace ownership files.",
        ));
    }
    for protected in [
        source.to_path_buf(),
        PathBuf::from(format!("{}-wal", source.display())),
        PathBuf::from(format!("{}-shm", source.display())),
        PathBuf::from(format!("{}-lock", source.display())),
    ] {
        let protected = resolved(&protected);
        #[cfg(windows)]
        let same = protected
            .to_string_lossy()
            .eq_ignore_ascii_case(&destination.to_string_lossy());
        #[cfg(not(windows))]
        let same = protected == destination;
        if same {
            return Err(StoreError::invalid(
                "Choose an export file outside the live workspace database files.",
            ));
        }
    }
    Ok(())
}

fn backup_database(source: &Path, destination: &Path) -> Result<()> {
    let _budget=crate::sqlite::input::Guard::new();
    validate_database_export_destination(source, destination)?;
    let parent = destination
        .parent()
        .ok_or_else(|| StoreError::invalid("Choose a destination folder."))?;
    let temporary = parent.join(format!(".capy-workspace-backup-{}", new_id()));
    let result = (|| -> Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(&temporary)
            .map_err(|e| StoreError::new(ErrorKind::FailedWrite, e.to_string()))?;
        let unavailable =
            |e: std::io::Error| StoreError::new(ErrorKind::Unavailable, e.to_string());
        let database = rusqlite::Connection::open_with_flags(
            std::fs::canonicalize(source).map_err(unavailable)?,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        crate::sqlite::input::install(&database)?;
        {
            let mut target=rusqlite::Connection::open(std::fs::canonicalize(&temporary).map_err(unavailable)?)?;
            crate::sqlite::input::install(&target)?;
            let backup=rusqlite::backup::Backup::new(&database,&mut target)?;
            loop {
                crate::sqlite::input::work()?;
                match backup.step(128)? {
                    rusqlite::backup::StepResult::Done=>break,
                    rusqlite::backup::StepResult::More=>(),
                    _=>std::thread::sleep(std::time::Duration::from_millis(10)),
                }
            }
        }
        drop(database);
        // Windows FlushFileBuffers requires a writable handle. SQLite has
        // closed its backup connection; reopen without truncating its result.
        std::fs::OpenOptions::new()
            .write(true)
            .open(&temporary)
            .and_then(|f| f.sync_all())
            .map_err(|e| StoreError::new(ErrorKind::FailedWrite, e.to_string()))?;
        std::fs::rename(&temporary, destination)
            .map_err(|e| StoreError::new(ErrorKind::FailedWrite, e.to_string()))?;
        #[cfg(unix)]
        std::fs::File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|e| StoreError::new(ErrorKind::FailedWrite, e.to_string()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

impl std::future::IntoFuture for StoreReply {
    type Output = Result<StoreResponse>;
    type IntoFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Self::Output>>>;
    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let Self {receiver,permit}=self;
            let result=receiver.recv().await.unwrap_or_else(|_| {
                Err(StoreError::new(
                    ErrorKind::Unavailable,
                    "Workspace storage worker stopped.",
                ))
            });
            drop(permit);result
        })
    }
}
impl WorkspaceStore for StoreWorker {
    fn retire_owner(&self, owner: &Owner) {
        let Ok(mut retiring)=self.0.retiring.lock() else {return;};
        if !retiring.insert((owner.id.clone(),owner.epoch.clone())) {return;}
        let (reply, _) = async_channel::bounded(1);
        let _ = self
            .0
            .sender
            .as_ref()
            .unwrap()
            .send((WorkerRequest::RetireOwner(owner.clone()), reply,None));
    }
    async fn execute(&self, request: StoreRequest) -> Result<StoreResponse> {
        self.request(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::IntoFuture;
    #[test]
    fn abandoned_requests_and_unread_replies_keep_their_admission() {
        let (sender,receiver)=mpsc::channel();
        let worker=StoreWorker(Arc::new(Worker {
            sender:Some(sender),thread:None,
            admission:Arc::new(admission::Admission::new(1,4096)),
            retiring:Default::default(),
        }));
        let first=worker.request(StoreRequest::List);
        drop(first);
        assert!(pollster::block_on(worker.request(StoreRequest::List).into_future()).is_err());
        let (_,reply,permit)=receiver.recv().unwrap();
        assert!(reply.try_send(Ok(StoreResponse::List(Vec::new()))).is_err());
        drop(permit);
        let second=worker.request(StoreRequest::List);
        let (_,reply,permit)=receiver.recv().unwrap();
        reply.try_send(Ok(StoreResponse::List(Vec::new()))).unwrap();drop(permit);
        assert!(pollster::block_on(worker.request(StoreRequest::List).into_future()).is_err());
        assert!(pollster::block_on(second.into_future()).is_ok());
        let third=worker.request(StoreRequest::List);
        let (_,_,permit)=receiver.recv().unwrap();drop(third);drop(permit);
    }
    #[test]
    fn saturated_storage_keeps_coalesced_owner_retirement_after_accepted_writes() {
        let (sender,receiver)=mpsc::channel();
        let worker=StoreWorker(Arc::new(Worker {
            sender:Some(sender),thread:None,
            admission:Arc::new(admission::Admission::new(1,4096)),
            retiring:Default::default(),
        }));
        let first=worker.request(StoreRequest::List);let owner=Owner::fresh();
        worker.retire_owner(&owner);worker.retire_owner(&owner);
        assert!(matches!(receiver.recv().unwrap().0,WorkerRequest::Store(StoreRequest::List)));
        assert!(matches!(receiver.recv().unwrap().0,WorkerRequest::RetireOwner(actual) if actual==owner));
        assert!(receiver.try_recv().is_err());drop(first);
    }
}
