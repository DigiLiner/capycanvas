#[path="../photo_reader/sandbox.rs"]
mod sandbox;
use std::{io::{BufReader,Write},os::fd::{FromRawFd,AsRawFd},sync::atomic::AtomicBool};

fn run()->Result<(),String> {
    let args=std::env::args().skip(1).map(|v|v.parse::<usize>().map_err(|e|e.to_string())).collect::<Result<Vec<_>,_>>()?;
    let [source_bytes,codec_bytes,dimension,allowance,parent]=args.as_slice() else {return Err("Invalid photo decoder arguments".into());};
    sandbox::parent_death(*parent)?;
    let limits=layer_color::photo::DecodeLimits {source_bytes:*source_bytes,codec_bytes:*codec_bytes,dimension:u32::try_from(*dimension).map_err(|e|e.to_string())?};
    let input=unsafe {std::fs::File::from_raw_fd(0)};
    if !input.metadata().map_err(|e|e.to_string())?.is_file() || unsafe {libc::fcntl(input.as_raw_fd(),libc::F_GETFL)}&libc::O_ACCMODE!=libc::O_RDONLY {return Err("Photo decoder requires read-only file input".into());}
    sandbox::restrict(*allowance)?;
    let cancel=AtomicBool::new(false);
    let photo=layer_color::photo::read_photo_detailed_with_cancel(BufReader::new(input),limits,&cancel)?;
    let depth=photo.source.interpretation.depth;
    let document=layer_color::photo_project(photo.source,photo.metadata,
        layer_core::DocumentNames {paint:"Photo".into(),paper:"Paper".into()},depth)?;
    let editor=layer_core::Editor::new(document);
    let capture=editor.capture(1,Default::default()).map_err(|e|e.to_string())?;
    let package=layer_core::package::codec::PreparedPackage::prepare(&capture,None,&cancel)?;
    let mut output=std::io::stdout().lock();output.write_all(&[u8::from(photo.first_frame)|u8::from(photo.primary_image)<<1]).map_err(|e|e.to_string())?;
    package.write(&mut output,&cancel)
}
fn main() {
    if let Err(error)=run() {eprintln!("{}",error.chars().take(1024).collect::<String>());std::process::exit(1);}
}
