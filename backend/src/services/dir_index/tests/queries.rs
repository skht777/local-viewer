use super::common::{make_args, setup};

#[test]
fn child_countが正しく返る() {
    let (idx, _tmp) = setup();

    let args = make_args(
        "/data",
        "/data",
        "m",
        vec![("sub1", 1_000_000)],
        vec![("a.jpg", 100, 2_000_000), ("b.png", 200, 3_000_000)],
    );
    idx.ingest_walk_entry(&args).unwrap();

    assert_eq!(idx.child_count("m").unwrap(), 3);
    assert_eq!(idx.child_count("nonexistent").unwrap(), 0);
}

#[test]
fn preview_entriesが画像とアーカイブを返す() {
    let (idx, _tmp) = setup();

    let args = make_args(
        "/data",
        "/data",
        "m",
        vec![("subdir", 1_000_000)],
        vec![
            ("photo.jpg", 100, 2_000_000),
            ("readme.txt", 50, 3_000_000),
            ("comic.zip", 500, 4_000_000),
            ("movie.mp4", 1000, 5_000_000),
        ],
    );
    idx.ingest_walk_entry(&args).unwrap();

    let previews = idx.preview_entries("m", 10).unwrap();
    let kinds: Vec<&str> = previews.iter().map(|e| e.kind.as_str()).collect();
    // directory と other (txt) は含まれない
    assert!(!kinds.contains(&"directory"));
    assert!(!kinds.contains(&"other"));
    assert_eq!(previews.len(), 3); // photo.jpg, comic.zip, movie.mp4
}

#[test]
fn first_entry_by_kindがarchiveを優先して返す() {
    let (idx, _tmp) = setup();

    let args = make_args(
        "/data/photos",
        "/data",
        "m1",
        vec![],
        vec![
            ("image1.jpg", 100, 1_000_000_000),
            ("archive.zip", 200, 2_000_000_000),
            ("doc.pdf", 300, 3_000_000_000),
        ],
    );
    idx.ingest_walk_entry(&args).unwrap();

    // archive が最初に見つかる
    let entry = idx.first_entry_by_kind("m1/photos", "archive").unwrap();
    assert!(entry.is_some());
    assert_eq!(entry.unwrap().name, "archive.zip");
}

#[test]
fn first_entry_by_kindで該当なしはnoneを返す() {
    let (idx, _tmp) = setup();

    let args = make_args(
        "/data/photos",
        "/data",
        "m1",
        vec![],
        vec![("image1.jpg", 100, 1_000_000_000)],
    );
    idx.ingest_walk_entry(&args).unwrap();

    let entry = idx.first_entry_by_kind("m1/photos", "archive").unwrap();
    assert!(entry.is_none());
}
