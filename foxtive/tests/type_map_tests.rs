mod common;

use foxtive::container::TypeMap;

#[test]
fn new_map_is_empty() {
    let map = TypeMap::new();
    assert!(map.is_empty());
    assert_eq!(map.len(), 0);
}

#[test]
fn default_creates_empty_map() {
    let map = TypeMap::default();
    assert!(map.is_empty());
}

#[test]
fn insert_and_retrieve_single_type() {
    let mut map = TypeMap::new();
    map.insert(42u32);
    assert_eq!(*map.get::<u32>().unwrap(), 42);
}

#[test]
fn insert_multiple_distinct_types() {
    let mut map = TypeMap::new();
    map.insert(42u32);
    map.insert("hello".to_string());
    map.insert(true);
    map.insert(1.234567f64);

    assert_eq!(*map.get::<u32>().unwrap(), 42);
    assert_eq!(*map.get::<String>().unwrap(), "hello".to_string());
    assert!(*map.get::<bool>().unwrap());
    assert_eq!(*map.get::<f64>().unwrap(), 1.234567);
    assert_eq!(map.len(), 4);
}

#[test]
fn get_missing_type_returns_none() {
    let map = TypeMap::new();
    assert_eq!(map.get::<u32>(), None);
    assert_eq!(map.get::<String>(), None);
}

#[test]
fn insert_same_type_twice_returns_old_value() {
    let mut map = TypeMap::new();
    assert!(map.insert(1u32).is_none());
    assert_eq!(map.insert(2u32), Some(1));
    assert_eq!(*map.get::<u32>().unwrap(), 2);
    assert_eq!(map.len(), 1);
}

#[test]
fn contains_returns_true_for_inserted_types() {
    let mut map = TypeMap::new();
    assert!(!map.contains::<u32>());
    map.insert(42u32);
    assert!(map.contains::<u32>());
    assert!(!map.contains::<String>());
}

#[test]
fn remove_returns_value_and_clears_slot() {
    let mut map = TypeMap::new();
    map.insert(42u32);
    assert_eq!(map.remove::<u32>(), Some(42));
    assert!(!map.contains::<u32>());
    assert_eq!(map.remove::<u32>(), None);
}

#[test]
fn remove_from_empty_map_returns_none() {
    let mut map = TypeMap::new();
    assert_eq!(map.remove::<u32>(), None);
}

#[test]
fn get_mut_allows_in_place_modification() {
    let mut map = TypeMap::new();
    map.insert(vec![1, 2, 3]);
    if let Some(v) = map.get_mut::<Vec<i32>>() {
        v.push(4);
    }
    assert_eq!(*map.get::<Vec<i32>>().unwrap(), vec![1, 2, 3, 4]);
}

#[test]
fn get_mut_returns_none_for_missing_type() {
    let mut map = TypeMap::new();
    assert!(map.get_mut::<u32>().is_none());
}

#[test]
fn len_tracks_insertions_and_removals() {
    let mut map = TypeMap::new();
    assert_eq!(map.len(), 0);

    map.insert(1u32);
    assert_eq!(map.len(), 1);

    map.insert("hi".to_string());
    assert_eq!(map.len(), 2);

    map.remove::<u32>();
    assert_eq!(map.len(), 1);
}

#[test]
fn different_wrapper_types_are_distinct() {
    let mut map = TypeMap::new();
    map.insert(42u32);
    map.insert(vec![1u32, 2, 3]);
    map.insert(Box::new(99u32));

    assert_eq!(*map.get::<u32>().unwrap(), 42);
    assert_eq!(*map.get::<Vec<u32>>().unwrap(), vec![1, 2, 3]);
    assert_eq!(*map.get::<Box<u32>>().unwrap(), Box::new(99));
    assert_eq!(map.len(), 3);
}

#[test]
fn custom_struct_as_service() {
    struct UserService {
        base_url: String,
    }

    struct CacheService {
        ttl_seconds: u64,
    }

    let mut map = TypeMap::new();
    map.insert(UserService {
        base_url: "https://api.example.com".into(),
    });
    map.insert(CacheService { ttl_seconds: 300 });

    let user_svc = map.get::<UserService>().unwrap();
    assert_eq!(user_svc.base_url, "https://api.example.com");

    let cache_svc = map.get::<CacheService>().unwrap();
    assert_eq!(cache_svc.ttl_seconds, 300);
}
