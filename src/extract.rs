use crate::{P2File, P2Subfile, HPAK, PK2D, PKAC, GroupedFiles, BErr, FileType};
use crate::iohelper::{
	IOHelper, IOManager, FileQueueEntry, RelPath
};
use crate::util::{UnwrapError, TryUnwrap};
use bytes::{Bytes, Buf};
use std::{
	mem::replace,
	convert::{TryFrom, TryInto},
	str,
	ffi::OsString
};
use nintendo_lz::decompress;

pub trait Parse {
	fn parse(bytes: &[u8]) -> Self;
}

impl Parse for P2File {
	fn parse(mut buf: &[u8]) -> Self {
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
		let subfiles = partials.iter().enumerate().map(|(i, f)| {
			let offset = f.offset.unwrap() as usize;
			let len = f.len.unwrap() as usize;
			let bytes = Bytes::copy_from_slice(&buf[offset..offset + len]);
			P2Subfile {
				index: i as u16,
				content: bytes,
				name: f.name.clone(),
				compressed: f.compressed.unwrap(),
			}
		}).collect();
		P2File{subfiles}
	}
}

#[derive(Default, Clone)]
struct PartialP2File {
	offset: Option<u32>,
	len: Option<u32>,
	name: Option<String>,
	compressed: Option<bool>
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

impl Parse for GroupedFiles {
	fn parse(orig_buf: &[u8]) -> GroupedFiles {
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
		file_groups
	}
}

fn make_file_table() -> [Option<Vec<Bytes>>; 8] { // lmao
	[Some(Vec::new()), Some(Vec::new()), Some(Vec::new()), Some(Vec::new()), Some(Vec::new()), Some(Vec::new()), Some(Vec::new()), Some(Vec::new())]
}

pub fn handle_file(mut file: FileQueueEntry, helper: &IOHelper) -> Result<(), BErr> {
	let ty = file.get_or_guess_type();
	if file.content.is_empty() {
		println!("Ignoring empty file {:?}", file.path)
	}
	//println!("Guessed type {:?} for {:?}", ty, file.path);
	match ty {
		FileType::P2 => {
			//println!("extract p2 {:?}", file.path);
			helper.create_dir(&file.path)?;
			for mut p2f in P2File::parse(&file.content).subfiles {
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
			let files = GroupedFiles::parse(&file.content);
			let parsed = AssetBundle::from_filegroups(files, ty)?;
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
			let parsed = GroupedFiles::parse(&file.content);
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

#[derive(Debug)]
enum AssetBundle {
	HPAK(HPAK),
	PK2D(PK2D)
}

impl AssetBundle {
	pub fn get_type_map(&self) -> Option<[(FileType, &[Bytes]); 8]> {
		match self {
			Self::HPAK(h) => Some(h.get_type_map()),
			Self::PK2D(p) => Some(p.get_type_map())
		}
	}
	
	pub fn from_filegroups(files: GroupedFiles, ty: FileType) -> Result<Self, BErr> {
		match ty {
			FileType::HPAK => Ok(Self::HPAK(files.into())),
			FileType::PK2D => Ok(Self::PK2D(files.into())),
			_ => Err(format!("attempted to read assetbundle with non-assetbundle type {:?}", ty).into())
		}
	}
}

