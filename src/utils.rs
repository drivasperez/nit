use std::path::{Component, Path, PathBuf};

pub fn bytes_to_hex_string(bytes: &[u8]) -> Result<String, std::fmt::Error> {
    use core::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(s, "{:02x}", byte)?;
    }

    Ok(s)
}

pub fn add_extension(path: &mut std::path::PathBuf, extension: impl AsRef<std::path::Path>) {
    match path.extension() {
        Some(ext) => {
            let mut ext = ext.to_os_string();
            ext.push(".");
            ext.push(extension.as_ref());
            path.set_extension(ext)
        }
        None => path.set_extension(extension.as_ref()),
    };
}

/// Determines from a file's mode whether it's executable or not.
pub fn is_executable(mode: u32) -> bool {
    mode & 0o111 != 0
}

pub fn drain_to_array<T: Default + Copy, const N: usize>(data: &mut Vec<T>) -> [T; N] {
    let mut arr = [T::default(); N];
    let drain = data.drain(0..N);

    for (i, item) in drain.into_iter().enumerate() {
        arr[i] = item;
    }

    arr
}

// https://github.com/Manishearth/pathdiff/blob/master/src/lib.rs
pub fn diff_paths<P, B>(path: P, base: B) -> Option<PathBuf>
where
    P: AsRef<Path>,
    B: AsRef<Path>,
{
    let path = path.as_ref();
    let base = base.as_ref();

    if path.is_absolute() != base.is_absolute() {
        if path.is_absolute() {
            Some(PathBuf::from(path))
        } else {
            None
        }
    } else {
        let mut ita = path.components();
        let mut itb = base.components();
        let mut comps: Vec<Component> = vec![];
        loop {
            match (ita.next(), itb.next()) {
                (None, None) => break,
                (Some(a), None) => {
                    comps.push(a);
                    comps.extend(ita.by_ref());
                    break;
                }
                (None, _) => comps.push(Component::ParentDir),
                (Some(a), Some(b)) if comps.is_empty() && a == b => (),
                (Some(a), Some(b)) if b == Component::CurDir => comps.push(a),
                (Some(_), Some(b)) if b == Component::ParentDir => return None,
                (Some(a), Some(_)) => {
                    comps.push(Component::ParentDir);
                    for _ in itb {
                        comps.push(Component::ParentDir);
                    }
                    comps.push(a);
                    comps.extend(ita.by_ref());
                    break;
                }
            }
        }
        Some(comps.iter().map(|c| c.as_os_str()).collect())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn drain_array() {
        let mut v = vec![0, 1, 2, 3, 4, 5];
        let arr: [u8; 3] = drain_to_array(&mut v);

        assert_eq!(arr, [0, 1, 2]);
        assert_eq!(v, vec![3, 4, 5]);

        let arr: [u8; 3] = drain_to_array(&mut v);
        assert_eq!(arr, [3, 4, 5]);
        assert_eq!(v, vec![]);
    }
}
