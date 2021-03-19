mod threads;
use threads::*;
use crate::{
	FileType, BErr,
	meta::{MetaRef, FileMeta}
};
use std::{
	io::{prelude::*, BufWriter},
	ffi::{OsString, OsStr},
	io,
	path::PathBuf,
	fs::{read_dir, create_dir_all, File},
};
use bytes::Bytes;
use crossbeam_channel::{Sender, Receiver};

pub struct IOManager {
	helper: IOHelper,
	pool: JankyThreadPool<FileQueueEntryInternal>
}

impl IOManager {
	pub fn new<T>(in_root: PathBuf, out_root: PathBuf, setup_fn: impl Fn(IOHelper) -> T + Sync + 'static + Send + Clone, file_handler: impl Fn(FileQueueEntry, MetaRef<FileMeta>, &T) + Send + 'static + Sync + Clone) -> Self {
		let (ic, oc) = (in_root.clone(), out_root.clone());
		let pool = JankyThreadPool::new(4, move |iqe: FileQueueEntryInternal, hlp| {
			let entry = iqe.entry;
			let mref = iqe.meta_ref;
			file_handler(entry, mref, hlp)
		}, move |s| {
			setup_fn(IOHelper {
				in_root: in_root.clone(), out_root: out_root.clone(), file_tx: Some(s)
			})
		});
		IOManager {
			helper: IOHelper {
				in_root: ic, out_root: oc,
				file_tx: Some(pool.task_sender())
			},
			pool
		}
	}
	
	pub fn get_helper(&self) -> &IOHelper {
		&self.helper
	}
	
	pub fn join(self) {
		self.pool.join();
	}
}

#[derive(Clone)]
pub struct IOHelper {
	in_root: PathBuf,
	out_root: PathBuf,
	file_tx: Option<TaskSender<FileQueueEntryInternal>>
}

impl IOHelper {
	pub fn read_file(&self, path: &RelPath) -> io::Result<Bytes> {
		let mut file = File::open(path.resolve(self.in_root.clone()))?;
		let mut buf = Vec::with_capacity(file.metadata()?.len() as usize);
		file.read_to_end(&mut buf)?; // why does this only take vec
		Ok(Bytes::from(buf))
	}
	
	pub fn read_dir(&self, path: &RelPath) -> io::Result<impl Iterator<Item = io::Result<RelPath>>> {
		let path = path.clone();
		read_dir(path.resolve(self.in_root.clone())).map(move |iter| {
			iter.map(move |res| {
				res.map(|p| {
					if let Some(subpath) = p.path().file_name() { 
						let mut parent = path.clone();
						parent.push(subpath.to_str().unwrap().into());
						parent
					} else {
						path.clone()
					}
				})
			})
		})
	}

	pub fn write_file(&self, path: &RelPath, content: &[u8]) -> io::Result<()> {
		let mut path = path.clone();
		let syspath = path.resolve(self.out_root.clone());
		path.pop();
		self.create_dir(&path)?;
		let mut writer = File::create(syspath)?;
		writer.write_all(content)
	}
	
	pub fn create_dir(&self, path: &RelPath) -> io::Result<()> {
		create_dir_all(path.resolve(self.out_root.clone()))
	}
	
	pub fn queue_or_write(&self, entry: FileQueueEntry, meta_ref: MetaRef<FileMeta>) -> Result<(), BErr> {
		if entry.get_or_guess_type().still_packed() {
			self.file_tx.as_ref().unwrap().send(FileQueueEntryInternal{entry, meta_ref})?;
			Ok(())
		} else {
			meta_ref.submit(FileMeta::OtherFile(entry.path.peek()));
			Ok(self.write_file(&entry.path, &entry.content)?)
		}
	}
	
	pub fn is_dir(&self, path: &RelPath) -> bool {
		path.resolve(self.in_root.clone()).is_dir()
	}
	
	pub fn new(in_path: PathBuf, out_path: PathBuf) -> Self {
		IOHelper {
			in_root: in_path,
			out_root: out_path,
			file_tx: None
		}
	}
}

#[derive(Clone, Debug)]
pub struct RelPath {
	path: Vec<String>
}

impl RelPath {
	pub fn push(&mut self, p: String) {
		if !p.is_empty() {
			self.path.push(p)
		}
	}
	
	pub fn prepend(self, e: String) -> Self {
		let mut p = RelPath::new();
		p.push(e);
		for s in self.path {
			p.push(s);
		}
		p
	}
	
	pub fn pop(&mut self) -> Option<String> {
		self.path.pop()
	}
	
	pub fn resolve(&self, mut to: PathBuf) -> PathBuf {
		for p in &self.path {
			to.push(p.clone())
		}
		to
	}
	
	pub fn new() -> Self {
		RelPath {
			path: Vec::new()
		}
	}
	
	pub fn peek(&self) -> String {
		if self.path.is_empty() {
			String::from("")
		} else {
			self.path.last().cloned().unwrap() // why does this convert &OsString to &OsStr? no clue
		}
	}
}

pub struct FileQueueEntry {
	pub content: Bytes,
	pub path: RelPath,
	pub type_hint: Option<FileType>,
	pub compression_hint: Option<bool>,
}

struct FileQueueEntryInternal {
	entry: FileQueueEntry,
	meta_ref: MetaRef<FileMeta>
}

unsafe impl Send for FileQueueEntryInternal{}

impl FileQueueEntry {
	pub fn get_or_guess_type(&self) -> FileType {
		if let Some(ty) = self.type_hint {
			ty
		} else {
			FileType::guess_from(&self.content, self.compression_hint.unwrap_or(true))
		}
	}
}

