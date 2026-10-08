use std::{path::PathBuf, sync::Arc};
use vcxsrv_wsl_manager::{
    authority::{Authority, AuthorityError, AuthorityStore},
    instance::InstanceGuard,
    storage::ProtectedDirectory,
};
fn fixture(name: &str) -> (PathBuf, ProtectedDirectory) {
    let scope = format!("task6-{}-{name}", std::process::id());
    let path = std::env::temp_dir().join(&scope);
    let guard = Arc::new(InstanceGuard::acquire(&scope).unwrap());
    let directory = ProtectedDirectory::open_or_create(&path, guard).unwrap();
    (path, directory)
}
fn golden() -> Vec<u8> {
    let mut bytes = vec![255, 255, 0, 0, 0, 1, b'1', 0, 18];
    bytes.extend_from_slice(b"MIT-MAGIC-COOKIE-1");
    bytes.extend_from_slice(&[0, 16]);
    bytes.extend(0u8..16);
    bytes
}
#[test]
fn canonical_record_uses_big_endian_lengths_and_exact_cookie() {
    let bytes = golden();
    let authority = Authority::decode_xauthority(&bytes, 1).unwrap();
    assert_eq!(authority.encode_xauthority(), bytes);
    assert_eq!(bytes.len(), 45);
    assert!(Authority::decode_xauthority(&bytes, 2).is_err());
    for end in 0..bytes.len() {
        assert!(Authority::decode_xauthority(&bytes[..end], 1).is_err());
    }
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(Authority::decode_xauthority(&extra, 1).is_err());
    let mut wrong = bytes.clone();
    wrong[0] = 0;
    assert!(Authority::decode_xauthority(&wrong, 1).is_err());
    let mut wrong = bytes;
    wrong[8] = 255;
    assert!(Authority::decode_xauthority(&wrong, 1).is_err());
}
#[test]
fn authority_reuses_existing_record_and_never_recreates_for_owned_server() {
    let (path, directory) = fixture("reuse");
    drop(directory.create_new("authority", &golden()).unwrap());
    let store = AuthorityStore::new(&directory);
    let one = store.load_or_create(1).unwrap();
    let two = store.load_or_create(1).unwrap();
    assert_eq!(one.encode_xauthority(), golden());
    assert_eq!(two.encode_xauthority(), golden());
    drop(one);
    drop(two);
    let identity = directory.read("authority", 64).unwrap().identity();
    directory.remove("authority", identity).unwrap();
    drop(directory.create_new("ownership.json", b"{}").unwrap());
    assert!(matches!(
        store.load_or_create(1),
        Err(AuthorityError::MissingForOwnedServer)
    ));
    assert!(!path.join("authority").exists());
    drop(directory);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn generated_authority_is_canonical_for_display99_and_rejects_unsafe_existing_file() {
    let (path, directory) = fixture("generate");
    let store = AuthorityStore::new(&directory);
    let authority = store.load_or_create(99).unwrap();
    let bytes = authority.encode_xauthority();
    assert_eq!(&bytes[..8], &[255, 255, 0, 0, 0, 2, b'9', b'9']);
    assert_eq!(bytes.len(), 46);
    assert!(Authority::decode_xauthority(&bytes, 99).is_ok());
    assert!(store.load_or_create(100).is_err());
    drop(authority);
    let identity = directory.read("authority", 64).unwrap().identity();
    directory.remove("authority", identity).unwrap();
    std::fs::write(path.join("authority"), golden()).unwrap();
    assert!(store.load_or_create(1).is_err());
    drop(directory);
    std::fs::remove_dir_all(path).unwrap();
}
