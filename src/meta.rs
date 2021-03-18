use std::{
	collections::HashMap,
};
use serde::{Serialize, Deserialize};
use crate::{
	P2File, PKAC, PK2D, HPAK
};
// Metadata needed to properly re-pack things.
#[derive(Serialize, Deserialize, Clone)]
pub enum FileMeta {
	Directory(DirectoryMeta),
	P2(P2Meta),
	LZ(LZMeta),
	NamedP2(NamedP2Meta),
	OtherFile(String), // unpacked name of the file
	HPAK(HPAKMeta),
	PK2D(PK2DMeta),
	PKAC(PKACMeta),
	EmptyFile,
	Uninitialized
}

impl MetaSubmit for FileMeta {
	type MetaRefCollection = ();
	unsafe fn on_submit(&mut self) {}
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DirectoryMeta {
	unpacked_name: String,
	files: HashMap<String, FileMeta>
}

impl DirectoryMeta {
	pub fn create(unpacked_name: String) -> Self {
		DirectoryMeta{unpacked_name, files: HashMap::new()}
	}
	
	pub fn add(&mut self, key: String) {
		self.files.insert(key, FileMeta::Uninitialized);
	}
	
	pub fn get_files(&self) -> &HashMap<String, FileMeta> {
		&self.files
	}
	
	pub fn get_unpacked_name(&self) -> &str {
		&self.unpacked_name
	}
}

impl MetaSubmit for DirectoryMeta {
	type MetaRefCollection = HashMap<String, MetaRef<FileMeta>>;
	unsafe fn on_submit(&mut self) -> Self::MetaRefCollection {
		let mut map = HashMap::new();
		for (name, file) in &mut self.files {
			map.insert(name.clone(), MetaRef::new(file));
		}
		map
	}
}

impl From<DirectoryMeta> for FileMeta {
	fn from(other: DirectoryMeta) -> Self {
		Self::Directory(other)
	}
}

impl DirectoryMeta {
	pub fn from(other: &[String], unpacked_name: String) -> Self {
		let mut map = HashMap::new();
		for f in other {
			map.insert(f.clone(), FileMeta::Uninitialized);
		}
		DirectoryMeta {
			files: map,
			unpacked_name
		}
	}
}

#[derive(Serialize, Deserialize, Clone)]
pub struct P2Meta {
	unpacked_name: String,
	files: Vec<P2SubfileMeta>
}

impl MetaSubmit for P2Meta {
	type MetaRefCollection = Vec<MetaRef<FileMeta>>;
	unsafe fn on_submit(&mut self) -> Self::MetaRefCollection {
		self.files.iter_mut().map(|sf| MetaRef::new(&mut sf.file)).collect()
	}
}

impl From<P2Meta> for FileMeta {
	fn from(other: P2Meta) -> Self {
		Self::P2(other)
	}
}

impl P2Meta {
	pub fn from(other: &P2File, unpacked_name: String) -> Self {
		let mut files = Vec::with_capacity(other.subfiles.len());
		for subfile in &other.subfiles {
			files.push(P2SubfileMeta {
				compressed: subfile.compressed,
				file: FileMeta::Uninitialized
			});
		}
		P2Meta {
			files, unpacked_name
		}
	}
	
	pub fn get_files(&self) -> &[P2SubfileMeta] {
		&self.files
	}
	
	pub fn get_unpacked_name(&self) -> &str {
		&self.unpacked_name
	}
}

#[derive(Serialize, Deserialize, Clone)]
pub struct P2SubfileMeta {
	compressed: bool,
	file: FileMeta
}

impl P2SubfileMeta {
	pub fn is_compressed(&self) -> bool {
		self.compressed
	}
	
	pub fn get_file(&self) -> &FileMeta {
		&self.file
	}
}

#[derive(Serialize, Deserialize, Clone)]
pub struct NamedP2Meta {
	unpacked_name: String,
	files: Vec<(String, P2SubfileMeta)>
}

impl MetaSubmit for NamedP2Meta {
	type MetaRefCollection = Vec<(String, MetaRef<FileMeta>)>;
	unsafe fn on_submit(&mut self) -> Self::MetaRefCollection {
		self.files.iter_mut().map(|(n, m)| (n.clone(), MetaRef::new(&mut m.file))).collect()
	}
}

impl From<NamedP2Meta> for FileMeta {
	fn from(other: NamedP2Meta) -> Self {
		Self::NamedP2(other)
	}
}

impl NamedP2Meta {
	pub fn from(other: &P2File, unpacked_name: String) -> Self {
		let mut files = Vec::with_capacity(other.subfiles.len());
		for subfile in &other.subfiles {
			files.push((subfile.name.as_ref().unwrap().into(), P2SubfileMeta {
				compressed: subfile.compressed,
				file: FileMeta::Uninitialized
			}));
		}
		NamedP2Meta {
			files, unpacked_name
		}
	}
	
	pub fn get_unpacked_name(&self) -> &str {
		&self.unpacked_name
	}
	
	pub fn get_files(&self) -> &[(String, P2SubfileMeta)] {
		&self.files
	}
}

#[derive(Serialize, Deserialize, Clone)]
pub struct HPAKMeta {
	unpacked_name: String,
	nsbca_files: Vec<FileMeta>,
	nsbva_files: Vec<FileMeta>,
	nsbma_files: Vec<FileMeta>,
	nsbtp_files: Vec<FileMeta>,
	nsbta_files: Vec<FileMeta>,
	unknown5_files: Vec<FileMeta>,
	unknown6_files: Vec<FileMeta>,
	nsbmd_files: Vec<FileMeta>
}

impl MetaSubmit for HPAKMeta {
	type MetaRefCollection = [Vec<MetaRef<FileMeta>>; 8];
	unsafe fn on_submit(&mut self) -> Self::MetaRefCollection {
		[
			get_refs_vec(&mut self.nsbca_files),
			get_refs_vec(&mut self.nsbva_files),
			get_refs_vec(&mut self.nsbma_files),
			get_refs_vec(&mut self.nsbtp_files),
			get_refs_vec(&mut self.nsbta_files),
			get_refs_vec(&mut self.unknown5_files),
			get_refs_vec(&mut self.unknown6_files),
			get_refs_vec(&mut self.nsbmd_files),
		]
	}
}

impl From<HPAKMeta> for FileMeta {
	fn from(other: HPAKMeta) -> Self {
		Self::HPAK(other)
	}
}

impl HPAKMeta {
	pub fn from(other: &HPAK, name: String) -> Self {
		HPAKMeta {
			unpacked_name: name,
			nsbca_files: vec![FileMeta::Uninitialized; other.nsbca.len()],
			nsbva_files: vec![FileMeta::Uninitialized; other.nsbva.len()],
			nsbma_files: vec![FileMeta::Uninitialized; other.nsbma.len()],
			nsbtp_files: vec![FileMeta::Uninitialized; other.nsbtp.len()],
			nsbta_files: vec![FileMeta::Uninitialized; other.nsbta.len()],
			unknown5_files: vec![FileMeta::Uninitialized; other.unknown5.len()],
			unknown6_files: vec![FileMeta::Uninitialized; other.unknown6.len()],
			nsbmd_files: vec![FileMeta::Uninitialized; other.nsbmd.len()],
		}
	}
	
	pub fn get_nsbca(&self) -> &[FileMeta] {
		&self.nsbca_files
	}
	pub fn get_nsbva(&self) -> &[FileMeta] {
		&self.nsbva_files
	}
	pub fn get_nsbma(&self) -> &[FileMeta] {
		&self.nsbma_files
	}
	pub fn get_nsbtp(&self) -> &[FileMeta] {
		&self.nsbtp_files
	}
	pub fn get_nsbta(&self) -> &[FileMeta] {
		&self.nsbta_files
	}
	pub fn get_unknown5(&self) -> &[FileMeta] {
		&self.unknown5_files
	}
	pub fn get_unknown6(&self) -> &[FileMeta] {
		&self.unknown6_files
	}
	pub fn get_nsbmd(&self) -> &[FileMeta] {
		&self.nsbmd_files
	}
	
	pub fn get_unpacked_name(&self) -> &str {
		&self.unpacked_name
	}
}

unsafe fn get_refs_vec(vec: &mut Vec<FileMeta>) -> Vec<MetaRef<FileMeta>> {
	vec.iter_mut().map(|m| MetaRef::new(m)).collect()
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PK2DMeta {
	unpacked_name: String,
	nclr_files: Vec<FileMeta>,
	ncgr_files: Vec<FileMeta>,
	unknown2_files: Vec<FileMeta>,
	ncer_files: Vec<FileMeta>,
	unknown4_files: Vec<FileMeta>,
	nanr_files: Vec<FileMeta>,
	nscr_files: Vec<FileMeta>,
	unknown7_files: Vec<FileMeta>
}

impl MetaSubmit for PK2DMeta {
	type MetaRefCollection = [Vec<MetaRef<FileMeta>>; 8];
	unsafe fn on_submit(&mut self) -> Self::MetaRefCollection {
		[
			get_refs_vec(&mut self.nclr_files),
			get_refs_vec(&mut self.ncgr_files),
			get_refs_vec(&mut self.unknown2_files),
			get_refs_vec(&mut self.ncer_files),
			get_refs_vec(&mut self.unknown4_files),
			get_refs_vec(&mut self.nanr_files),
			get_refs_vec(&mut self.nscr_files),
			get_refs_vec(&mut self.unknown7_files),
		]
	}
}

impl From<PK2DMeta> for FileMeta {
	fn from(other: PK2DMeta) -> Self {
		Self::PK2D(other)
	}
}

impl PK2DMeta {
	pub fn from(other: &PK2D, name: String) -> Self {
		PK2DMeta {
			unpacked_name: name,
			nclr_files: vec![FileMeta::Uninitialized; other.nclr.len()],
			ncgr_files: vec![FileMeta::Uninitialized; other.ncgr.len()],
			unknown2_files: vec![FileMeta::Uninitialized; other.unknown2.len()],
			ncer_files: vec![FileMeta::Uninitialized; other.ncer.len()],
			unknown4_files: vec![FileMeta::Uninitialized; other.unknown4.len()],
			nanr_files: vec![FileMeta::Uninitialized; other.nanr.len()],
			nscr_files: vec![FileMeta::Uninitialized; other.nscr.len()],
			unknown7_files: vec![FileMeta::Uninitialized; other.unknown7.len()]
		}
	}
	
	pub fn get_nclr(&self) -> &[FileMeta] {
		&self.nclr_files
	}
	pub fn get_ncgr(&self) -> &[FileMeta] {
		&self.ncgr_files
	}
	pub fn get_unknown2(&self) -> &[FileMeta] {
		&self.unknown2_files
	}
	pub fn get_ncer(&self) -> &[FileMeta] {
		&self.ncer_files
	}
	pub fn get_unknown4(&self) -> &[FileMeta] {
		&self.unknown4_files
	}
	pub fn get_nanr(&self) -> &[FileMeta] {
		&self.nanr_files
	}
	pub fn get_nscr(&self) -> &[FileMeta] {
		&self.nscr_files
	}
	pub fn get_unknown7(&self) -> &[FileMeta] {
		&self.unknown7_files
	}
	
	pub fn get_unpacked_name(&self) -> &str {
		&self.unpacked_name
	}
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PKACMeta {
	unpacked_name: String,
	files: Vec<(String, FileMeta)>
}

impl MetaSubmit for PKACMeta {
	type MetaRefCollection = Vec<(String, MetaRef<FileMeta>)>;
	unsafe fn on_submit(&mut self) -> Self::MetaRefCollection {
		self.files.iter_mut().map(|(n, m)| (n.clone(), MetaRef::new(m))).collect()
	} 
}

impl From<PKACMeta> for FileMeta {
	fn from(other: PKACMeta) -> Self {
		Self::PKAC(other)
	}
}

impl PKACMeta {
	pub fn from(other: &PKAC, unpacked_name: String) -> Self {
		PKACMeta {
			unpacked_name,
			files: other.files.iter().map(|(n, _)| (n.into(), FileMeta::Uninitialized)).collect()
		}
	}
	
	pub fn get_unpacked_name(&self) -> &str {
		&self.unpacked_name
	}
	
	pub fn get_files(&self) -> &[(String, FileMeta)] {
		&self.files
	}
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LZMeta {
	lz_type: LZType,
	file: Box<FileMeta>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum LZType {
	LZ10, LZ11
}

impl MetaSubmit for LZMeta {
	type MetaRefCollection = MetaRef<FileMeta>;
	unsafe fn on_submit(&mut self) -> Self::MetaRefCollection {
		MetaRef::new(&mut *self.file)
	}
}

impl From<LZMeta> for FileMeta {
	fn from(other: LZMeta) -> Self {
		Self::LZ(other)
	}
}

impl LZMeta {
	pub fn new(ty: LZType) -> Self {
		LZMeta {
			lz_type: ty, file: Box::new(FileMeta::Uninitialized)
		}
	}
	
	pub fn get_lz_type(&self) -> LZType {
		self.lz_type
	}
	
	pub fn get_file(&self) -> &Box<FileMeta> {
		&self.file
	}
}

// this isn't thread safe but rustc won't put Send or Sync on native pointers. this matters so we can't send them out of a handler.
pub struct MetaRef<T> {
	ptr: *mut T
}

pub trait MetaSubmit {
	type MetaRefCollection;
	unsafe fn on_submit(&mut self) -> Self::MetaRefCollection; // POINTERS RETURNED MUST BE HEAP POINTERS OR OTHERWISE STATIC!
}

impl<T> MetaRef<T> {
	pub fn submit<U: MetaSubmit + Into<T>>(self, mut u: U) -> U::MetaRefCollection {
		let refs = unsafe{u.on_submit()};
		unsafe {
			*self.ptr = u.into()
		}; // safety is enforced by the creation of MetaRefs
		refs
	}
	
	pub unsafe fn new(ptr: *mut T) -> Self {
		MetaRef{ptr}
	}
}
