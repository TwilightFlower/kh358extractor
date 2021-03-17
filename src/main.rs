mod iohelper;
mod util;
mod magic;
use std::{
	env::args,
	io,
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

type BErr = Box<dyn std::error::Error + 'static>;

fn main() -> Result<(), BErr> {
	let args = &args().collect::<Vec<String>>();
	let target = PathBuf::from(&args[1]);
	let out = PathBuf::from(&args[2]);
	let manager = IOManager::new(target, out, |f, h| {handle_file(f, h).unwrap()});
	handle_dir(manager.get_helper(), &RelPath::new())?;
	manager.join();
	Ok(())
}

fn handle_file(mut file: FileQueueEntry, helper: &IOHelper) -> Result<(), BErr> {
	let ty = file.get_or_guess_type();
	if file.content.is_empty() {
		println!("Ignoring empty file {:?}", file.path)
	}
	//println!("Guessed type {:?} for {:?}", ty, file.path);
	match ty {
		FileType::P2 => {
			//println!("extract p2 {:?}", file.path);
			helper.create_dir(&file.path)?;
			for mut p2f in P2File::parse(&file.content) {
				if p2f.content.is_empty() {
					println!("Ignoring empty P2 subfile at index {} in {:?}", p2f.index, file.path);
					continue;
				}
				p2f.decompress()?;
				let t_guess = FileType::guess_from(&p2f.content, false);
				let name = format!("{}.{}", p2f.suggest_name(), t_guess.get_extension());
				let mut p = file.path.clone();
				p.push(OsString::from(name));
				helper.queue_or_write(FileQueueEntry {
					path: p,
					content: p2f.content,
					type_hint: Some(t_guess),
					compression_hint: Some(false)
				})?
			}
		},
		FileType::LZ => {
			//println!("decompress {:?}", file.path);
			file.content = Bytes::from(decompress(&mut file.content.reader())?);
			file.type_hint = None;
			file.compression_hint = Some(false);
			helper.queue_or_write(file)?
		},
		FileType::HPAK | FileType::PK2D => {
			//println!("extract asset store {:?}", file.path);
			helper.create_dir(&file.path)?;
			let parsed = parse_asset_container(&file.content);
			let map = parsed.get_type_map().try_unwrap().map_err(|x| x.strip_data())?;
			for (typ, group) in &map {
				for (i, data) in group.iter().enumerate() {
					let name = OsString::from(format!("{}.{}", i, typ.get_extension()));
					let mut new_path = file.path.clone();
					new_path.push(name);
					helper.queue_or_write(FileQueueEntry {
						path: new_path,
						content: data.clone(),
						type_hint: Some(*typ),
						compression_hint: None
					})?
				}
			}
		},
		FileType::PKAC => {
			helper.create_dir(&file.path)?;
			let parsed = parse_asset_container(&file.content).try_unwrap_other()?;
			let pkac: PKAC = parsed.try_into()?;
			for (name, subfile) in pkac.files {
				let ty = FileType::guess_from(&subfile, true); // unsure if can be compressed or not
				let name = OsString::from(format!("{}.{}", name, ty.get_extension()));
				let mut new_path = file.path.clone();
				new_path.push(name);
				helper.queue_or_write(FileQueueEntry {
					path: new_path,
					content: subfile,
					type_hint: Some(ty),
					compression_hint: None
				})?;
			}
		}
		_ => ()
	}
	Ok(())
}

fn handle_dir(helper: &IOHelper, in_path: &RelPath) -> io::Result<()> {
	for path in helper.read_dir(in_path)? {
		let path = path?;
		if helper.is_dir(&path) {
			handle_dir(&helper, &path)
		} else {
			helper.queue_or_write(FileQueueEntry {
				path: path.clone(),
				content: helper.read_file(&path)?,
				type_hint: None, compression_hint: None
			})
		}?;
	}
	Ok(())
}

#[derive(Default, Clone)]
struct PartialP2File {
	offset: Option<u32>,
	len: Option<u32>,
	name: Option<String>,
	compressed: Option<bool>
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

struct P2File {
	index: u16,
	compressed: bool,
	content: Bytes,
	name: Option<String>,
}

#[derive(Debug)]
struct HPAK {
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

impl From<[Option<Vec<Bytes>>; 8]> for HPAK {
	fn from(mut other: [Option<Vec<Bytes>>; 8]) -> Self {
		HPAK {
			nsbca: replace(&mut other[0], None).unwrap(),
			nsbva: replace(&mut other[1], None).unwrap(),
			nsbma: replace(&mut other[2], None).unwrap(),
			nsbtp: replace(&mut other[3], None).unwrap(),
			nsbta: replace(&mut other[4], None).unwrap(),
			unknown5: replace(&mut other[5], None).unwrap(),
			unknown6: replace(&mut other[6], None).unwrap(),
			nsbmd: replace(&mut other[7], None).unwrap()
		}
	}
}

#[derive(Debug)]
struct PK2D {
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

impl From<[Option<Vec<Bytes>>; 8]> for PK2D {
	fn from(mut other: [Option<Vec<Bytes>>; 8]) -> Self {
		PK2D {
			nclr: replace(&mut other[0], None).unwrap(),
			ncgr: replace(&mut other[1], None).unwrap(),
			unknown2: replace(&mut other[2], None).unwrap(),
			ncer: replace(&mut other[3], None).unwrap(),
			unknown4: replace(&mut other[4], None).unwrap(),
			nanr: replace(&mut other[5], None).unwrap(),
			nscr: replace(&mut other[6], None).unwrap(),
			unknown7: replace(&mut other[7], None).unwrap()
		}
	}
}

#[derive(Debug)]
struct PKAC {
	files: Vec<(String, Bytes)>
}

impl TryFrom<[Option<Vec<Bytes>>; 8]> for PKAC {
	type Error = BErr;
	fn try_from(other: [Option<Vec<Bytes>>; 8]) -> Result<Self, Self::Error> {
		if let Some(nametable) = &other[0] {
			if let Some(files) = &other[1] {
				let nametable = read_nametable(&nametable[0])?;
				if nametable.len() >= files.len() {
					Ok(PKAC{
						files: nametable.iter().enumerate().map(|(i, name)| {(name.clone(), files[i].clone())}).collect()
					})
				} else {
					Err("Name table shorter than file table".into())
				}
			} else {
				Err("PKAC format requires index 1 be present".into())
			}
		} else {
			Err("PKAC format requires index 0 be present as a name table".into())
		}
	}
}

fn read_nametable(orig_buf: &[u8]) -> Result<Vec<String>, BErr> {
	let mut buf = orig_buf;
	let mut names = Vec::new();
	let n = buf.get_u16_le();
	for _ in 0..n {
		let offset = buf.get_u16_le();
		let str_buf = &orig_buf[offset as usize..];
		let first_nul = str_buf.iter().position(|x| *x == 0).unwrap();
		names.push(String::from_utf8(orig_buf[offset as usize..offset as usize + first_nul].to_vec())?);
	}
	Ok(names)
}

#[derive(Debug)]
enum AssetBundle {
	HPAK(HPAK),
	PK2D(PK2D),
	Other([Option<Vec<Bytes>>; 8])
}

impl AssetBundle {
	pub fn get_type_map(&self) -> Option<[(FileType, &[Bytes]); 8]> {
		match self {
			Self::HPAK(h) => Some(h.get_type_map()),
			Self::PK2D(p) => Some(p.get_type_map()),
			_ => None
		}
	}
	
	pub fn try_unwrap_other(self) -> Result<[Option<Vec<Bytes>>; 8], UnwrapError<AssetBundle>> {
		if let Self::Other(data) = self {
			Ok(data)
		} else {
			Err(UnwrapError::create(self, "called try_unwrap_other on non-other value".into()))
		}
	}
}

impl P2File {
	fn parse(mut buf: &[u8]) -> Vec<P2File> {
		let orig_buf = buf;
		buf.advance(2); // the P2 header
		let num_files = buf.get_u16_le();
		let has_name_table = ((num_files >> 8) & 0x80) != 0;
		let num_files = num_files & !(0x8000);
		buf.advance(8); // padding
		let header_size = buf.get_u32_le();
		let mut partials: Vec<PartialP2File> = vec![Default::default(); num_files as usize];
		for i in 0..num_files {
			let offset = buf.get_u16_le() as u32;
			let offset = offset * 0x200  + header_size;
			partials[i as usize].offset = Some(offset);
		}
		buf.advance((num_files as usize & 1) * 2); // there's padding if odd number of files
		for i in 0..num_files {
			let p = buf.get_u32_le();
			let len = p & 0xFFFFFF;
			let compressed = ((p >> 24) & 0xFF) == 0x80;
			partials[i as usize].len = Some(len);
			partials[i as usize].compressed = Some(compressed);
		}
		if has_name_table {
			for i in 0..num_files {
				let mut string_buf = [0; 8];
				buf.copy_to_slice(&mut string_buf);
				let name = str::from_utf8(&string_buf).expect("filename not utf8!?").trim_matches(char::from(0));
				partials[i as usize].name = Some(name.into());
			}
		}
		let buf = orig_buf;
		partials.iter().enumerate().map(|(i, f)| {
			let offset = f.offset.unwrap() as usize;
			let len = f.len.unwrap() as usize;
			let bytes = Bytes::copy_from_slice(&buf[offset..offset + len]);
			P2File {
				index: i as u16,
				content: bytes,
				name: f.name.clone(),
				compressed: f.compressed.unwrap(),
			}
		}).collect()
	}
	
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

fn parse_asset_container(orig_buf: &[u8]) -> AssetBundle {
	let mut buf = orig_buf;
	let magic = buf.get_u32_le();
	buf.get_u32(); // padding
	let mut file_groups = make_file_table();
	for f in &mut file_groups {
		let files = f.as_mut().unwrap();
		let f_info_offset = buf.get_u32_le() as usize;
		if f_info_offset == 0xFFFFFFFF {
			continue; // used to indicate empty
		}
		let mut f_info_buf = &orig_buf[f_info_offset..];
		let n_files = f_info_buf.get_u32_le();
		let mut len_buf = &f_info_buf[n_files as usize * 4..];
		for _ in 0..n_files {
			let offset = f_info_buf.get_u32_le() as usize;
			let length = len_buf.get_u32_le() as usize;
			files.push(Bytes::copy_from_slice(&orig_buf[offset..offset + length]))
		}
	}
	match magic {
		HPAK_MAGIC => AssetBundle::HPAK(file_groups.into()),
		PK2D_MAGIC => AssetBundle::PK2D(file_groups.into()),
		_ => AssetBundle::Other(file_groups)
	}
}

fn make_file_table() -> [Option<Vec<Bytes>>; 8] { // lmao
	[Some(Vec::new()), Some(Vec::new()), Some(Vec::new()), Some(Vec::new()), Some(Vec::new()), Some(Vec::new()), Some(Vec::new()), Some(Vec::new())]
}
