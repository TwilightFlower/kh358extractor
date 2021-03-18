use crate::{P2File, P2Subfile, HPAK, PK2D, PKAC, GroupedFiles, BErr, FileType};
use crate::iohelper::{
	IOHelper, FileQueueEntry
};
use crate::util::{TryUnwrap};
use crate::meta::{
	FileMeta, MetaRef, P2Meta, NamedP2Meta, LZMeta, LZType, HPAKMeta, PK2DMeta, PKACMeta
};
use bytes::{Bytes, Buf};
use std::{
	mem::{replace},
	convert::{TryFrom, TryInto},
	str
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
		P2File {
			subfiles, named: has_name_table
		}
	}
}

#[derive(Default, Clone)]
struct PartialP2File {
	offset: Option<u32>,
	len: Option<u32>,
	name: Option<String>,
	compressed: Option<bool>
}

impl From<[Vec<Bytes>; 8]> for HPAK {
	fn from(other: [Vec<Bytes>; 8]) -> Self {
		let [nsbca, nsbva, nsbma, nsbtp, nsbta, unknown5, unknown6, nsbmd] = other;
		HPAK{nsbca, nsbva, nsbma, nsbtp, nsbta, unknown5, unknown6, nsbmd}
	}
}

impl From<[Vec<Bytes>; 8]> for PK2D {
	fn from(other: [Vec<Bytes>; 8]) -> Self {
		let [nclr, ncgr, unknown2, ncer, unknown4, nanr, nscr, unknown7] = other;
		PK2D{nclr, ncgr, unknown2, ncer, unknown4, nanr, nscr, unknown7}
	}
}

impl TryFrom<[Vec<Bytes>; 8]> for PKAC {
	type Error = BErr;
	fn try_from(other: [Vec<Bytes>; 8]) -> Result<Self, Self::Error> {
		if !other[0].is_empty() {
			if !other[1].is_empty() {
				let nametable = read_nametable(&other[0][0])?;
				let files = &other[1];
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
		let _magic = buf.get_u32_le();
		buf.get_u32(); // padding
		let mut file_groups = make_file_table();
		for files in &mut file_groups {
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

fn make_file_table() -> [Vec<Bytes>; 8] { // lmao
	[Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()]
}

pub fn handle_file(mut file: FileQueueEntry, meta_ref: MetaRef<FileMeta>, helper: &IOHelper) -> Result<(), BErr> {
	let ty = file.get_or_guess_type();
	if file.content.is_empty() {
		println!("Ignoring empty file {:?}", file.path)
	}
	//println!("Guessed type {:?} for {:?}", ty, file.path);
	match ty {
		FileType::P2 => {
			//println!("extract p2 {:?}", file.path);
			helper.create_dir(&file.path)?;
			let p2_container = P2File::parse(&file.content);
			let name = file.path.peek();
			let mut meta_refs = optioned_vec_of(if p2_container.named {
				let meta = meta_ref.submit(NamedP2Meta::from(&p2_container, name));
				let mut vec = Vec::with_capacity(meta.len());
				for (_, r) in meta {
					vec.push(r);
				}
				vec
			} else {
				meta_ref.submit(P2Meta::from(&p2_container, name))
			});
			//let meta_refs = meta.submit(mut u: U)
			for mut p2f in p2_container.subfiles {
				let meta_ref = replace(&mut meta_refs[p2f.index as usize], None).unwrap();
				if p2f.content.is_empty() {
					println!("Ignoring empty P2 subfile at index {} in {:?}", p2f.index, file.path);
					meta_ref.submit(FileMeta::EmptyFile);
					continue;
				}
				p2f.decompress()?;
				let t_guess = FileType::guess_from(&p2f.content, false);
				let name = format!("{}.{}", p2f.suggest_name(), t_guess.get_extension());
				let mut p = file.path.clone();
				p.push(name);
				helper.queue_or_write(FileQueueEntry {
					path: p,
					content: p2f.content,
					type_hint: Some(t_guess),
					compression_hint: Some(false)
				}, meta_ref)?
			}
		},
		FileType::LZ => {
			//println!("decompress {:?}", file.path);
			let lz_type = if file.content[0] == 0x10 {LZType::LZ10} else {LZType::LZ11};
			let meta_ref = meta_ref.submit(LZMeta::new(lz_type));
			file.content = Bytes::from(decompress(&mut file.content.reader())?);
			file.type_hint = None;
			file.compression_hint = Some(false);
			helper.queue_or_write(file, meta_ref)?
		},
		FileType::HPAK | FileType::PK2D => {
			//println!("extract asset store {:?}", file.path);
			helper.create_dir(&file.path)?;
			let files = GroupedFiles::parse(&file.content);
			let parsed = AssetBundle::from_filegroups(files, ty)?;
			let map = parsed.get_type_map().try_unwrap().map_err(|x| x.strip_data())?;
			let mut meta_refs = arrays_suck(parsed.submit_meta_to(meta_ref, file.path.peek()));
			for (i1, (typ, group)) in map.iter().enumerate() {
				for (i2, data) in group.iter().enumerate() {
					let name = format!("{}.{}", i2, typ.get_extension());
					let mut new_path = file.path.clone();
					new_path.push(name);
					helper.queue_or_write(FileQueueEntry {
						path: new_path,
						content: data.clone(),
						type_hint: Some(*typ),
						compression_hint: None
					}, replace(&mut meta_refs[i1][i2], None).unwrap())?
				}
			}
		},
		FileType::PKAC => {
			helper.create_dir(&file.path)?;
			let parsed = GroupedFiles::parse(&file.content);
			let pkac: PKAC = parsed.try_into()?;
			let meta = PKACMeta::from(&pkac, file.path.peek());
			let mut meta_refs = optioned_vec_of(meta_ref.submit(meta));
			for (i, (name, subfile)) in pkac.files.iter().enumerate() {
				let ty = FileType::guess_from(&subfile, true); // unsure if can be compressed or not
				let name = format!("{}.{}", name, ty.get_extension());
				let mut new_path = file.path.clone();
				new_path.push(name);
				helper.queue_or_write(FileQueueEntry {
					path: new_path,
					content: subfile.clone(),
					type_hint: Some(ty),
					compression_hint: None
				}, replace(&mut meta_refs[i], None).unwrap().1)?;
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
	
	pub fn submit_meta_to(&self, meta_ref: MetaRef<FileMeta>, unpacked_name: String) -> [Vec<MetaRef<FileMeta>>; 8] {
		match self {
			Self::HPAK(h) => meta_ref.submit(HPAKMeta::from(h, unpacked_name)),
			Self::PK2D(p) => meta_ref.submit(PK2DMeta::from(p, unpacked_name))
		}
	}
}

fn optioned_vec_of<T>(vec: Vec<T>) -> Vec<Option<T>> {
	let mut new_vec = Vec::with_capacity(vec.len());
	for t in vec {
		new_vec.push(Some(t));
	}
	new_vec
}

fn arrays_suck<T>(arr: [Vec<T>; 8]) -> [Vec<Option<T>>; 8] {
	let [v0, v1, v2, v3, v4, v5, v6, v7] = arr;
	[
		optioned_vec_of(v0),
		optioned_vec_of(v1),
		optioned_vec_of(v2),
		optioned_vec_of(v3),
		optioned_vec_of(v4),
		optioned_vec_of(v5),
		optioned_vec_of(v6),
		optioned_vec_of(v7)
	]
}

