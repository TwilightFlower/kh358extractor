use crate::{
	meta::{FileMeta, LZType},
	iohelper::{IOHelper, RelPath},
	P2File, PKAC, PK2D, HPAK, BErr, P2Subfile, GroupedFiles,
	magic::*,
	compression::safe_compress
};
use bytes::{Bytes, BytesMut, BufMut};
//use nintendo_lz::{CompressionLevel, compress};

const NULS: [u8; 2048] = [0; 2048]; // bunch of nul to copy
const FFS: [u8; 32] = [0xFF; 32];

pub fn pack_file(parent_unpacked_path: &RelPath, meta: &FileMeta, helper: &IOHelper) -> Result<Bytes, BErr> {
	let mut path = parent_unpacked_path.clone();
	match meta {
		FileMeta::OtherFile(name) => {
			path.push(name.clone());
			Ok(helper.read_file(&path)?)
		},
		FileMeta::EmptyFile => {
			Ok(Bytes::new())
		},
		FileMeta::LZ(lzm) => {
			/*let compression_level = match lzm.get_lz_type() {
				LZType::LZ10 => CompressionLevel::LZ10,
				LZType::LZ11 => CompressionLevel::LZ11(5)
			};*/
			let file = pack_file(&path, &*lzm.get_file(), helper)?;
			let compressed = safe_compress(&file)?;
			let bytes = Bytes::copy_from_slice(&compressed);
			Ok(bytes)
		}
		FileMeta::P2(p2m) => {
			//println!("{:?}", p2m);
			path.push(p2m.get_unpacked_name().into());
			let mut subfiles = Vec::with_capacity(p2m.get_files().len());
			let p2_files = p2m.get_files();
			for (i, file) in p2_files.iter().enumerate() {
				let mut buf = pack_file(&path, file.get_file(), helper)?;
				if file.is_compressed() && !buf.is_empty() {
					//let mut tbuf = BytesMut::with_capacity(buf.len()).writer();
					//compress(&buf, &mut tbuf, CompressionLevel::LZ11(5))?;
					//buf = tbuf.into_inner().freeze();
					let compressed = safe_compress(&buf)?;
					buf = Bytes::copy_from_slice(&compressed);
				}
				
				subfiles.push(P2Subfile {
					index: i as u16,
					compressed: file.is_compressed(),
					content: buf,
					name: None
				});
			}
			Ok(P2File {
				named: false,
				subfiles
			}.into())
		}
		FileMeta::NamedP2(p2m) => {
			path.push(p2m.get_unpacked_name().into());
			let mut subfiles = Vec::with_capacity(p2m.get_files().len());
			let p2_files = p2m.get_files();
			for (i, (name, file)) in p2_files.iter().enumerate() {
				let mut buf = pack_file(&path, file.get_file(), helper)?;
				if file.is_compressed() && !buf.is_empty() {
					/*let mut tbuf = BytesMut::with_capacity(buf.len()).writer();
					compress(&buf, &mut tbuf, CompressionLevel::LZ11(5))?;
					buf = tbuf.into_inner().freeze();*/
					let compressed = safe_compress(&buf)?;
					buf = Bytes::copy_from_slice(&compressed);
				}
				subfiles.push(P2Subfile {
					index: i as u16,
					compressed: file.is_compressed(),
					content: buf,
					name: Some(name.clone())
				});
			}
			Ok(P2File {
				named: true,
				subfiles
			}.into())
		},
		FileMeta::PKAC(pkac_meta) => {
			path.push(pkac_meta.get_unpacked_name().into());
			let mut files = Vec::new();
			for (name, file) in pkac_meta.get_files() {
				files.push((name.into(), pack_file(&path, file, helper)?));
			}
			let bytes: Bytes = GFWrapper(PKAC{files}.into()).into();
			let mut buf = BytesMut::new();
			buf.put_u32_le(PKAC_MAGIC);
			buf.put_u32_le(0);
			buf.put(bytes);
			Ok(buf.freeze())
		}
		FileMeta::HPAK(hpak_meta) => {
			path.push(hpak_meta.get_unpacked_name().into());
			let hpak = HPAK {
				nsbca: load_metas(&path, hpak_meta.get_nsbca(), helper)?,
				nsbva: load_metas(&path, hpak_meta.get_nsbva(), helper)?,
				nsbma: load_metas(&path, hpak_meta.get_nsbma(), helper)?,
				nsbtp: load_metas(&path, hpak_meta.get_nsbtp(), helper)?,
				nsbta: load_metas(&path, hpak_meta.get_nsbta(), helper)?,
				unknown5: load_metas(&path, hpak_meta.get_unknown5(), helper)?,
				unknown6: load_metas(&path, hpak_meta.get_unknown6(), helper)?,
				nsbmd: load_metas(&path, hpak_meta.get_nsbmd(), helper)?
			};
			let bytes: Bytes = GFWrapper(hpak.into()).into();
			let mut buf = BytesMut::new();
			buf.put_u32_le(HPAK_MAGIC);
			buf.put_u32_le(0);
			buf.put(bytes);
			Ok(buf.freeze())
		},
		FileMeta::PK2D(pk2d_meta) => {
			path.push(pk2d_meta.get_unpacked_name().into());
			let pk2d = PK2D {
				nclr: load_metas(&path, pk2d_meta.get_nclr(), helper)?,
				ncgr: load_metas(&path, pk2d_meta.get_ncgr(), helper)?,
				unknown2: load_metas(&path, pk2d_meta.get_unknown2(), helper)?,
				ncer: load_metas(&path, pk2d_meta.get_ncer(), helper)?,
				unknown4: load_metas(&path, pk2d_meta.get_unknown4(), helper)?,
				nanr: load_metas(&path, pk2d_meta.get_nanr(), helper)?,
				nscr: load_metas(&path, pk2d_meta.get_nscr(), helper)?,
				unknown7: load_metas(&path, pk2d_meta.get_unknown7(), helper)?,
			};
			let bytes: Bytes = GFWrapper(pk2d.into()).into();
			let mut buf = BytesMut::new();
			buf.put_u32_le(PK2D_MAGIC);
			buf.put_u32_le(0);
			buf.put(bytes);
			Ok(buf.freeze())
		},
		FileMeta::Directory(dir_meta) => {
			path.push(dir_meta.get_unpacked_name().into());
			helper.create_dir(&path)?;
			for (name, file) in dir_meta.get_files() {
				if let FileMeta::Directory(_) = file {
					pack_file(&path, file, helper)?;
				} else {
					let mut f_path = path.clone();
					f_path.push(name.clone());
					helper.write_file(&f_path, &pack_file(&path, file, helper)?)?;
				}
			}
			Ok(Bytes::new()) // directories return empty, since they are side-effect based rather than pure parsing/serializing
		},
		FileMeta::Uninitialized => {
			println!("Uninitialized metadata at {:?}", path);
			Ok(Bytes::new())
		}
	}
}

fn load_metas(parent_path: &RelPath, metas: &[FileMeta], helper: &IOHelper) -> Result<Vec<Bytes>, BErr> {
	let mut res = Vec::with_capacity(metas.len());
	for file in metas.iter().map(|m| pack_file(parent_path, m, helper)) {
		res.push(file?);
	}
	Ok(res)
}

impl Into<Bytes> for P2File {
	fn into(self) -> Bytes {
		let n_files = self.subfiles.len() as u16;
		let mut header_buf = BytesMut::new();
		header_buf.put_u16_le(P2_MAGIC);
		header_buf.put_u16_le(n_files | (0x8000 * self.named as u16));
		header_buf.put_u64_le(0); // padding
		let header_size_ptr = header_buf.len();
		header_buf.put_u32_le(0); // placeholder
		
		let mut contents_buf = BytesMut::new();
		let mut cur_offs = 0;
		for file in &self.subfiles {
			header_buf.put_u16_le((cur_offs >> 9) as u16);
			let len = file.content.len();
			let dist_from_block = 512 - (len & 511);
			cur_offs += len + dist_from_block;
			contents_buf.put(file.content.clone());
			if dist_from_block != 512 {
				contents_buf.put(&NULS[..dist_from_block]);
			}
		}
		let contents_buf = contents_buf.freeze();
		if n_files & 1 != 0 {
			header_buf.put_u16_le(0); // padding if odd
		}
		for file in &self.subfiles {
			let flag = 0x80000000 * file.compressed as u32;
			let value = flag | file.content.len() as u32;
			header_buf.put_u32_le(value);
		}
		if self.named {
			for file in &self.subfiles {
				let name = file.name.clone().unwrap();
				let name_bytes = name.as_bytes();
				header_buf.put(name_bytes);
				header_buf.put(&NULS[..8 - name_bytes.len()]);
			}
		}
		let mut header_size = header_buf.len() >> 9;
		let header_dist = 512 - (header_buf.len() & 511);
		if header_dist != 512 {
			header_size += 1;
			header_buf.put(&NULS[..header_dist]);
		}
		header_size <<= 9;
		let mut header_s_bytes = &mut header_buf[header_size_ptr..header_size_ptr + 4];
		header_s_bytes.put_u32_le(header_size as u32);
		header_buf.put(contents_buf);
		header_buf.freeze()
	}
}

impl Into<GroupedFiles> for HPAK {
	fn into(self) -> GroupedFiles {
		[
			self.nsbca,
			self.nsbva,
			self.nsbma,
			self.nsbtp,
			self.nsbta,
			self.unknown5,
			self.unknown6,
			self.nsbmd
		]
	}
}

impl Into<GroupedFiles> for PK2D {
	fn into(self) -> GroupedFiles {
		[
			self.nclr,
			self.ncgr,
			self.unknown2,
			self.ncer,
			self.unknown4,
			self.nanr,
			self.nscr,
			self.unknown7
		]
	}
}

impl Into<GroupedFiles> for PKAC {
	fn into(self) -> GroupedFiles {
		let mut names_buf = BytesMut::new();
		names_buf.put_u16_le(self.files.len() as u16);
		names_buf.put(&NULS[..self.files.len() * 2]);
		let mut files = Vec::with_capacity(self.files.len());
		let mut offset = names_buf.len();
		for (i, (name, file)) in self.files.iter().enumerate() {
			files.push(file.clone());
			let mut offset_loc = &mut names_buf[(i + 1) * 2..(i + 2) * 2];
			offset_loc.put_u16_le(offset as u16);
			names_buf.put(name.as_bytes());
			names_buf.put_u8(0); // null terminator
			offset += name.as_bytes().len() + 1;
		}
		[
			vec![names_buf.freeze()],
			files,
			Vec::new(),
			Vec::new(),
			Vec::new(),
			Vec::new(),
			Vec::new(),
			Vec::new()
		]
	}
}

struct GFWrapper(GroupedFiles);

impl Into<Bytes> for GFWrapper { // does not contain magic bytes or 4 padding!
	fn into(self) -> Bytes {
		let mut buf = BytesMut::new();
		buf.put(&FFS[..32]);
		let mut info_offsets = [0; 8];
		for (i, group) in self.0.iter().enumerate() {
			if !group.is_empty() {
				buf.put_u32_le(group.len() as u32);
				info_offsets[i] = buf.len();
				let mut info_loc = &mut buf[i * 4..]; 
				info_loc.put_u32_le(info_offsets[i] as u32 + 8); // account for missing magic + padding
				buf.put(&NULS[..4 * group.len()]);
				for f in group.iter() {
					buf.put_u32_le(f.len() as u32);
				}
			}
		}
		for (i, group) in self.0.iter().enumerate() {
			if !group.is_empty() {
				for (fi, file) in group.iter().enumerate() {
					let offset = buf.len() + 8;
					let mut offset_buf = &mut buf[info_offsets[i] + fi * 4..];
					offset_buf.put_u32_le(offset as u32);
					buf.put(&file[..]);
				}
			}
		}
		buf.freeze()
	}
}
