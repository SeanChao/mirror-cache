pub fn now() -> i64 {
    chrono::offset::Local::now().timestamp()
}

pub fn split_dirs(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(idx) => {
            let (l, r) = path.split_at(idx);
            (l, &r[1..])
        }
        None => ("", path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_dirs_two_level() {
        let s = "awesome/cache";
        let (l, r) = split_dirs(s);
        assert_eq!(l, "awesome");
        assert_eq!(r, "cache");
    }

    #[test]
    fn split_dirs_one_level() {
        let s = "42";
        let (l, r) = split_dirs(s);
        assert_eq!(l, "");
        assert_eq!(r, "42");
    }

    #[test]
    fn split_dirs_multi_level() {
        let s = "dead/beef/bread";
        let (l, r) = split_dirs(s);
        assert_eq!(l, &s[0..9]);
        assert_eq!(r, &s[10..]);
    }
}
