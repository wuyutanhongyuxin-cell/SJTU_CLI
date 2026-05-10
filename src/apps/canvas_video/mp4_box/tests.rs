//! mp4_box 单元测试：fixture mp4 解析 + box header 边界。

use super::parser::read_box_header;

#[test]
fn read_box_header_parses_size_and_type() {
    // box: size=12（含自身 8 字节 header）, type=ftyp, body 4 字节
    let bytes = [
        0x00, 0x00, 0x00, 0x0c, // size = 12
        b'f', b't', b'y', b'p', // type = ftyp
        0xde, 0xad, 0xbe, 0xef,
    ];
    let h = read_box_header(&bytes, 0).unwrap();
    assert_eq!(h.size, 12);
    assert_eq!(h.box_type, *b"ftyp");
    assert_eq!(h.header_len, 8);
    assert_eq!(h.body_start, 8);
}

#[test]
fn read_box_header_handles_largesize_64bit() {
    // size=1 → 后面 8 字节是真 size（large box）
    let bytes = [
        0x00, 0x00, 0x00, 0x01, // size = 1（信号）
        b'm', b'd', b'a', b't', 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10,
        0x00, // largesize = 4096
    ];
    let h = read_box_header(&bytes, 0).unwrap();
    assert_eq!(h.size, 4096);
    assert_eq!(h.box_type, *b"mdat");
    assert_eq!(h.header_len, 16);
    assert_eq!(h.body_start, 16);
}

#[test]
fn read_box_header_rejects_truncated_input() {
    let bytes = [0x00, 0x00, 0x00, 0x0c, b'f', b't']; // 只 6 字节，header 至少 8
    assert!(read_box_header(&bytes, 0).is_err());
}
