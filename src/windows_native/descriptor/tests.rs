use std::path::{Path, PathBuf};

#[test]
fn test_descriptor_parser() {
    let data: Vec<PathBuf> = Path::new("./etc/hidapi/windows/test/data")
        .read_dir()
        .unwrap()
        .map(|entry| entry
            .unwrap()
            .path())
        .filter(|entry| entry
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext == "pp_data"))
        .collect();

    for path in data {
        println!("{:?}", path);
    }
}

