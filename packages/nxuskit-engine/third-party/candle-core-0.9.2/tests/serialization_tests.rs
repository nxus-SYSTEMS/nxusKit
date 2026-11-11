use candle_core::{DType, Result, Tensor};

struct TmpFile(std::path::PathBuf);

impl TmpFile {
    fn create(base: &str) -> TmpFile {
        let filename = std::env::temp_dir().join(format!(
            "candle-{}-{}-{:?}",
            base,
            std::process::id(),
            std::thread::current().id(),
        ));
        TmpFile(filename)
    }
}

impl std::convert::AsRef<std::path::Path> for TmpFile {
    fn as_ref(&self) -> &std::path::Path {
        self.0.as_path()
    }
}

impl Drop for TmpFile {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).unwrap()
    }
}

#[test]
fn npy() -> Result<()> {
    let npy = Tensor::read_npy("tests/test.npy")?;
    assert_eq!(
        npy.to_dtype(DType::U8)?.to_vec1::<u8>()?,
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
    );
    Ok(())
}

#[test]
fn npz() -> Result<()> {
    let npz = Tensor::read_npz("tests/test.npz")?;
    assert_eq!(npz.len(), 2);
    assert_eq!(npz[0].0, "x");
    assert_eq!(npz[1].0, "x_plus_one");
    assert_eq!(
        npz[1].1.to_dtype(DType::U8)?.to_vec1::<u8>()?,
        [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
    );
    Ok(())
}

#[test]
fn npz_eager_and_lazy_filename_and_tensor_fidelity() -> Result<()> {
    use std::io::Write;

    let npy = TmpFile::create("filename.npy");
    let npz = TmpFile::create("filename.npz");
    Tensor::from_vec(vec![3u8, 5], 2, &candle_core::Device::Cpu)?.write_npy(&npy)?;
    let bytes = std::fs::read(&npy)?;
    let mut writer = zip::ZipWriter::new(std::fs::File::create(&npz)?);
    let options: zip::write::FileOptions<()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for name in ["weights.npy", "plain", "café.npy", "cafX.npy"] {
        writer.start_file(name, options)?;
        writer.write_all(&bytes)?;
    }
    writer.finish()?;
    // Hand-derived CP437 0x82 is é. It is not malformed UTF-8 that ZIP should
    // discard: both locked ZIP APIs deliberately support this legacy encoding.
    let mut archive = std::fs::read(&npz)?;
    let offsets: Vec<_> = archive
        .windows(b"cafX.npy".len())
        .enumerate()
        .filter_map(|(offset, name)| (name == b"cafX.npy").then_some(offset))
        .collect();
    assert_eq!(offsets.len(), 2, "local and central filenames");
    for offset in offsets {
        archive[offset + 3] = 0x82;
    }
    std::fs::write(&npz, archive)?;

    let eager = Tensor::read_npz(&npz)?;
    assert_eq!(eager.len(), 4);
    assert_eq!(
        eager
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["weights", "plain", "café", "café"],
    );
    for (_, tensor) in eager {
        assert_eq!(tensor.to_vec1::<u8>()?, [3, 5]);
    }
    let lazy = candle_core::npy::NpzTensors::new(&npz)?;
    let mut names = lazy
        .names()
        .into_iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    names.sort_unstable();
    assert_eq!(names, ["café", "plain", "weights"]);
    for name in names {
        assert_eq!(
            lazy.get(name)?.expect("selected tensor").to_vec1::<u8>()?,
            [3, 5]
        );
    }
    assert!(lazy.get("absent")?.is_none());
    Ok(())
}

#[test]
fn invalid_zip_archive_fails_without_partial_tensor_success() -> Result<()> {
    let path = TmpFile::create("invalid.zip");
    std::fs::write(&path, b"not a zip archive")?;
    assert!(matches!(
        Tensor::read_npz(&path),
        Err(candle_core::Error::Zip(_))
    ));
    assert!(matches!(
        candle_core::npy::NpzTensors::new(&path),
        Err(candle_core::Error::Zip(_)),
    ));
    assert!(matches!(
        candle_core::pickle::read_pth_tensor_info(&path, false, None),
        Err(candle_core::Error::Zip(_)),
    ));
    Ok(())
}

#[test]
fn safetensors() -> Result<()> {
    use candle_core::safetensors::Load;

    let tmp_file = TmpFile::create("st");
    let t = Tensor::arange(0f32, 24f32, &candle_core::Device::Cpu)?;
    t.save_safetensors("t", &tmp_file)?;
    // Load from file.
    let st = candle_core::safetensors::load(&tmp_file, &candle_core::Device::Cpu)?;
    let t2 = st.get("t").unwrap();
    let diff = (&t - t2)?.abs()?.sum_all()?.to_vec0::<f32>()?;
    assert_eq!(diff, 0f32);
    // Load from bytes.
    let bytes = std::fs::read(tmp_file)?;
    let st = candle_core::safetensors::SliceSafetensors::new(&bytes)?;
    let t2 = st.get("t").unwrap().load(&candle_core::Device::Cpu);
    let diff = (&t - t2)?.abs()?.sum_all()?.to_vec0::<f32>()?;
    assert_eq!(diff, 0f32);
    Ok(())
}
