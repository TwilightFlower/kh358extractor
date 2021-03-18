mod iohelper;
mod util;
mod magic;
mod extract;
mod meta;
mod pack;
use std::{
	env::args,
	io,
	io::Write,
	fs::File,
	fs,
	path::PathBuf,
	str,
	mem::replace,
	ffi::OsString,
	convert::{TryFrom, TryInto},
	fmt::{Debug},
};
use bytes::{
	Buf, Bytes
};
use nintendo_lz::decompress;
use iohelper::{
	IOHelper, IOManager, FileQueueEntry, RelPath
};
use crate::util::*;
use crate::magic::*;
use crate::meta::{DirectoryMeta, FileMeta, MetaRef};
use crate::pack::pack_file;
use ron::{ser, ser::PrettyConfig, de};

type BErr = Box<dyn std::error::Error + 'static>;
type GroupedFiles = [Vec<Bytes>; 8];

fn main() -> Result<(), BErr> {
	let args = &args().collect::<Vec<String>>();
	let action = String::from(&args[1]);
	let target = PathBuf::from(&args[2]);
	let out = PathBuf::from(&args[3]);
	let meta = String::from(&args[4]);
	match &action[..] {
		"pack" => {
			let meta: FileMeta = de::from_str(&fs::read_to_string(&meta)?)?;
			repack(target, out, &meta)?;
		}
		"extract" => {
			extract_tree(target, out, meta)?;
		}
		_ => {
			println!("invalid action {}", action)
		}
	}
	//extract_tree(target, out)?;
	
	Ok(())
}

fn repack(target: PathBuf, out: PathBuf, meta: &FileMeta) -> Result<(), BErr> {
	let helper = IOHelper::new(target, out);
	let path = RelPath::new();
	pack_file(&path, meta, &helper)?;
	Ok(())
}

fn extract_tree(target: PathBuf, out: PathBuf, meta: String) -> Result<(), BErr> {
	let manager = IOManager::new(target, out, |i| i, |f, m, h| {extract::handle_file(f, m, h).unwrap()});
	let mut meta_root = Box::new(FileMeta::Uninitialized);
	let meta_root_ref = unsafe{MetaRef::new(&mut *meta_root)};
	handle_extract_dir(manager.get_helper(), &RelPath::new(), meta_root_ref)?;
	manager.join();
	let config = PrettyConfig::new()
		.with_indentor("\t".into());
	let serialized = ser::to_string_pretty(&*meta_root, config)?;
	let mut metafile = File::create(meta)?;
	write!(metafile, "{}", serialized);
	Ok(())
}

fn handle_extract_dir(helper: &IOHelper, in_path: &RelPath, meta_ref: MetaRef<FileMeta>) -> Result<(), BErr> {
	let mut meta = DirectoryMeta::create(in_path.peek());
	for path in helper.read_dir(in_path)? {
		let path = path?;
		meta.add(path.peek());
	}
	let meta_refs = meta_ref.submit(meta);
	for (name, meta_ref) in meta_refs {
		let mut path = in_path.clone();
		path.push(name);
		if helper.is_dir(&path) {
			handle_extract_dir(helper, &path, meta_ref)
		} else {
			helper.queue_or_write(FileQueueEntry {
				path: path.clone(),
				content: helper.read_file(&path)?,
				type_hint: None, compression_hint: None
			}, meta_ref)
		}?
	}
	Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileType {
	P2,
	LZ,
	HPAK,
	PK2D,
	PKAC,
	OtherOrNotGuessable,
	NSBCA,
	NSBVA,
	NSBMA,
	NSBTP,
	NSBTA,
	NSBTX,
	Unknown5,
	Unknown6,
	NSBMD,
	NCLR,
	NCGR,
	Unknown0,
	Unknown1,
	Unknown2,
	Unknown3,
	NCER,
	Unknown4,
	NANR,
	NSCR,
	NFTR,
	Unknown7,
	SDAT
}

impl FileType {
	fn guess_from(mut buf: &[u8], could_be_compressed: bool) -> Self {
		if buf.len() < 4 {
			return FileType::OtherOrNotGuessable
		}
		let magic = buf.get_u32_le();
		let magic_16 = magic & 0xFFFF;
		let magic_8 = magic & 0xFF;
		match magic {
			HPAK_MAGIC => Self::HPAK,
			PK2D_MAGIC => Self::PK2D,
			PKAC_MAGIC => Self::PKAC,
			NSBMD_MAGIC => Self::NSBMD,
			NSBTX_MAGIC => Self::NSBTX,
			NSBCA_MAGIC => Self::NSBCA,
			NSBTP_MAGIC => Self::NSBTP,
			NSBTA_MAGIC => Self::NSBTA,
			NSBMA_MAGIC => Self::NSBMA,
			NSBVA_MAGIC => Self::NSBVA,
			NCGR_MAGIC => Self::NCGR,
			NCLR_MAGIC => Self::NCLR,
			NSCR_MAGIC => Self::NSCR,
			NFTR_MAGIC => Self::NFTR,
			NCER_MAGIC => Self::NCER,
			NANR_MAGIC => Self::NANR,
			SDAT_MAGIC => Self::SDAT,
			_ => {
				if magic_16 == P2_MAGIC as u32 {
					Self::P2
				} else if (magic_8 == 0x10 || magic_8 == 0x11) && could_be_compressed {
					Self::LZ
				} else {
					Self::OtherOrNotGuessable
				}
			}
		}
	}
	
	fn get_extension(&self) -> &'static str {
		match self {
			Self::SDAT => "sdat",
			Self::P2 => "p2",
			Self::LZ => "lz",
			Self::HPAK => "hpak",
			Self::PK2D => "pk2d",
			Self::PKAC => "pkac",
			Self::NSBCA => "nsbca",
			Self::NSBVA => "nsbva",
			Self::NSBMA => "nsbma",
			Self::NSBTP => "nsbtp",
			Self::NSBTA => "nsbta",
			Self::NSBTX => "nsbtx",
			Self::Unknown5 => "5.bin",
			Self::Unknown6 => "6.bin",
			Self::NSBMD => "nsbmd",
			Self::NCLR => "nclr",
			Self::NCGR => "ncgr",
			Self::Unknown0 => "0.bin",
			Self::Unknown1 => "1.bin",
			Self::Unknown2 => "2.bin",
			Self::Unknown3 => "3.bin",
			Self::NCER => "ncer",
			Self::Unknown4 => "4.bin",
			Self::NANR => "nanr",
			Self::NSCR => "nscr",
			Self::NFTR => "nftr",
			Self::Unknown7 => "7.bin",
			Self::OtherOrNotGuessable => "bin"
		}
	}
	
	fn still_packed(&self) -> bool {
		match self {
			Self::P2 | Self::LZ | Self::HPAK | Self::PK2D | Self::PKAC => true,
			_ => false 
		}
	}
}

pub struct P2File {
	named: bool,
	subfiles: Vec<P2Subfile>
}

struct P2Subfile {
	index: u16,
	compressed: bool,
	content: Bytes,
	name: Option<String>,
}

#[derive(Debug)]
pub struct HPAK {
	nsbca: Vec<Bytes>,
	nsbva: Vec<Bytes>,
	nsbma: Vec<Bytes>,
	nsbtp: Vec<Bytes>,
	nsbta: Vec<Bytes>,
	unknown5: Vec<Bytes>,
	unknown6: Vec<Bytes>,
	nsbmd: Vec<Bytes>
}

impl HPAK {
	pub fn get_type_map(&self) -> [(FileType, &[Bytes]); 8] {
		[
			(FileType::NSBCA, &self.nsbca),
			(FileType::NSBVA, &self.nsbva),
			(FileType::NSBMA, &self.nsbma),
			(FileType::NSBTP, &self.nsbtp),
			(FileType::NSBTA, &self.nsbta),
			(FileType::Unknown5, &self.unknown5),
			(FileType::Unknown6, &self.unknown6),
			(FileType::NSBMD, &self.nsbmd)
		]
	}
}



#[derive(Debug)]
pub struct PK2D {
	nclr: Vec<Bytes>,
	ncgr: Vec<Bytes>,
	unknown2: Vec<Bytes>,
	ncer: Vec<Bytes>,
	unknown4: Vec<Bytes>,
	nanr: Vec<Bytes>,
	nscr: Vec<Bytes>,
	unknown7: Vec<Bytes>
}

impl PK2D {
	pub fn get_type_map(&self) -> [(FileType, &[Bytes]); 8] {
		[
			(FileType::NCLR, &self.nclr),
			(FileType::NCGR, &self.ncgr),
			(FileType::Unknown2, &self.unknown2),
			(FileType::NCER, &self.ncer),
			(FileType::Unknown4, &self.unknown4),
			(FileType::NANR, &self.nanr),
			(FileType::NSCR, &self.nscr),
			(FileType::Unknown7, &self.unknown7)
		]
	}
}



#[derive(Debug)]
pub struct PKAC {
	files: Vec<(String, Bytes)>
}

impl P2Subfile {
	fn get_decompressed(&self) -> Result<Bytes, BErr> {
		if self.compressed {
			decompress(&mut self.content.clone().reader()).map(Bytes::from)
		} else {
			Ok(self.content.clone())
		}
	}
	
	fn decompress(&mut self) -> Result<(), BErr> {
		if self.compressed {
			let decompressed = decompress(&mut self.content.clone().reader()).map(Bytes::from)?;
			self.compressed = false;
			self.content = decompressed;
			
		}
		Ok(())
	}
	
	fn suggest_name(&self) -> String {
		if let Some(name) = &self.name {
			name.clone()
		} else {
			self.index.to_string()
		}
	}
}
