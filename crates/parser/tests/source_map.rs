use parser::module::SourceMap;
use std::{fs, time::{SystemTime, UNIX_EPOCH}};

#[test]
fn source_ids_resolve_back_to_the_original_file() {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("hydra_source_map_{}_{}.hydra", std::process::id(), stamp));
    fs::write(&path, "fn from_library() -> void {}\n").unwrap();

    let mut map = SourceMap::new();
    assert_eq!(map.load_file(&path).unwrap(), "fn from_library() -> void {}\n");
    let id = map.get_source_id(&path).unwrap();

    assert_eq!(map.path_for_id(id), Some(path.as_path()));
    assert_eq!(map.get_source(map.path_for_id(id).unwrap()), Some("fn from_library() -> void {}\n"));
    assert!(map.path_for_id(id + 1).is_none());

    fs::remove_file(path).unwrap();
}
