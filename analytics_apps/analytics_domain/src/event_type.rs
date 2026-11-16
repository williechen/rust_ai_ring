/// 事件代碼保留原始值，僅驗既有位元組長度及前後空白界線。
pub fn valid(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && value.trim() == value
}

/// 此切片只接受UTC的明確日期時間；真正日期由PostgreSQL型別再次核對。
pub fn utc_timestamp(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() < 20 || b.len() > 27 || b.last() != Some(&b'Z') {
        return false;
    }
    for i in [4, 7, 10, 13, 16] {
        if b[i]
            != match i {
                4 | 7 => b'-',
                10 => b'T',
                _ => b':',
            }
        {
            return false;
        }
    }
    if !(0..19)
        .filter(|i| ![4, 7, 10, 13, 16].contains(i))
        .all(|i| b[i].is_ascii_digit())
    {
        return false;
    }
    if &value[11..13] > "23" || &value[14..16] > "59" || &value[17..19] > "59" {
        return false;
    }
    b.len() == 20
        || (b[19] == b'.' && b.len() >= 22 && b[20..b.len() - 1].iter().all(u8::is_ascii_digit))
}
