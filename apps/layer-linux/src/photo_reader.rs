use layer_color::photo::{DecodeLimits,DecodedPhoto};
use layer_core::ProjectLimits;
use std::{fs::File,io::{Read,Seek,SeekFrom,Write},os::fd::AsRawFd,path::Path,process::{Child,ChildStdout,Command,Stdio},sync::{Arc,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};

const DEADLINE:Duration=Duration::from_secs(120);
struct Job(Child);
impl Drop for Job {fn drop(&mut self) {let _=self.0.kill();let _=self.0.wait();}}
struct Output<'a> {pipe:ChildStdout,cancelled:&'a AtomicBool,deadline:Instant}
impl Read for Output<'_> {
    fn read(&mut self,bytes:&mut [u8])->std::io::Result<usize> {
        if bytes.is_empty() {return Ok(0);}
        loop {
            active(self.cancelled,self.deadline).map_err(std::io::Error::other)?;
            let mut fd=libc::pollfd {fd:self.pipe.as_raw_fd(),events:libc::POLLIN,revents:0};
            let ready=unsafe {libc::poll(&mut fd,1,20)};
            if ready<0 {let error=std::io::Error::last_os_error();if error.kind()==std::io::ErrorKind::Interrupted {continue;}return Err(error);}
            if ready>0 {return self.pipe.read(bytes);}
        }
    }
}
fn active(cancelled:&AtomicBool,deadline:Instant)->Result<(),String> {
    if cancelled.load(Ordering::Acquire) {Err("Image import cancelled".into())}
    else if Instant::now()>=deadline {Err("Photo decoding timed out".into())}else {Ok(())}
}
pub(crate) fn read(input:impl Read+Seek,limits:DecodeLimits,cancelled:&AtomicBool)->Result<DecodedPhoto,String> {
    let executable=std::env::current_exe().map_err(|e|e.to_string())?;
    let helper=executable.ancestors().skip(1).take(2).map(|folder|folder.join("capy-photo-reader"))
        .find(|path|path.is_file()).ok_or("Photo decoder is unavailable")?;
    decode(input,limits,cancelled,&helper,DEADLINE)
}
pub(crate) fn decode(mut input:impl Read+Seek,limits:DecodeLimits,cancelled:&AtomicBool,helper:&Path,timeout:Duration)->Result<DecodedPhoto,String> {
    let deadline=Instant::now()+timeout;active(cancelled,deadline)?;
    let allowance=limits.source_bytes.saturating_add(limits.codec_bytes);
    let directory=layer_core::temp_files::directory()?;
    let mut encoded=layer_core::temp_files::anonymous(directory,"photo-input").map_err(|e|e.to_string())?;
    let mut buffer=[0;64*1024];let mut length=0usize;
    loop {
        active(cancelled,deadline)?;
        let count=input.read(&mut buffer).map_err(|e|e.to_string())?;if count==0 {break;}
        length=length.checked_add(count).filter(|length|*length<=limits.codec_bytes).ok_or("The encoded image exceeds the codec budget")?;
        encoded.write_all(&buffer[..count]).map_err(|e|e.to_string())?;
    }
    encoded.rewind().map_err(|e|e.to_string())?;
    let input=File::open(format!("/proc/self/fd/{}",encoded.as_raw_fd())).map_err(|e|e.to_string())?;
    let mut errors=layer_core::temp_files::anonymous(directory,"photo-errors").map_err(|e|e.to_string())?;
    let mut job=Job(Command::new(helper).env_clear()
        .args([limits.source_bytes.to_string(),limits.codec_bytes.to_string(),limits.dimension.to_string(),allowance.to_string(),std::process::id().to_string()])
        .stdin(input).stdout(Stdio::piped()).stderr(errors.try_clone().map_err(|e|e.to_string())?)
        .spawn().map_err(|e|format!("Photo decoder is unavailable: {e}"))?);
    let pipe=job.0.stdout.take().ok_or("Photo decoder output is unavailable")?;
    let mut output=Output {pipe,cancelled,deadline};let mut flags=[0];
    let result:Result<(layer_core::package::ImmutableBacking,ProjectLimits),String>=(|| {
        output.read_exact(&mut flags).map_err(|e|e.to_string())?;
        if flags[0]>3 {return Err("Invalid photo decoder response".into());}
        let project=ProjectLimits {asset_bytes:limits.source_bytes as u64+layer_core::color::source::MAX_PROFILE_BYTES as u64+layer_core::PhotoMetadata::MAX_BYTES as u64,dimension:limits.dimension,..Default::default()};
        let backing=layer_core::package::transport::spool(&mut output,directory,project.asset_bytes+project.metadata_bytes,cancelled)?;
        Ok((backing,project))
    })();
    if result.is_err() {let _=job.0.kill();}
    let status=loop {
        if let Some(status)=job.0.try_wait().map_err(|e|e.to_string())? {break status;}
        active(cancelled,deadline)?;std::thread::sleep(Duration::from_millis(10));
    };
    active(cancelled,deadline)?;
    if !status.success() {
        errors.seek(SeekFrom::Start(0)).map_err(|e|e.to_string())?;let mut detail=String::new();
        errors.take(8192).read_to_string(&mut detail).map_err(|e|e.to_string())?;
        return Err(if detail.trim().is_empty() {"Photo decoder failed".into()}else {detail});
    }
    let (backing,project)=result?;
    drop(job);
    let layer_core::package::codec::OpenOutcome::Candidate {artwork,..}=layer_core::package::codec::open(backing,project,cancelled)? else {return Err("Invalid photo decoder package".into());};
    let mut sources=artwork.paint.iter().filter_map(|(_,_,paint)|paint.base.as_ref().map(|base|base.image.storage().clone()));
    let source=sources.next().ok_or("Photo decoder returned no image")?;
    if sources.next().is_some() || source.extent.iter().any(|size|*size>limits.dimension)
        || source.resident_bytes()>limits.source_bytes {return Err("Photo decoder image exceeds admission".into());}
    source.validate()?;
    Ok(DecodedPhoto {source:Arc::unwrap_or_clone(source),metadata:(*artwork.metadata).clone(),first_frame:flags[0]&1!=0,primary_image:flags[0]&2!=0})
}
