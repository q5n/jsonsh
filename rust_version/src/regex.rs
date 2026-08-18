pub fn to_utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flags {
    pub global: bool,
    pub ignore_case: bool,
    pub multiline: bool,
}

impl Flags {
    pub fn parse(s: &str) -> Result<Self, String> {
        let mut f = Flags { global: false, ignore_case: false, multiline: false };
        for c in s.chars() {
            match c {
                'g' => f.global = true,
                'i' => f.ignore_case = true,
                'm' => f.multiline = true,
                _ => return Err(format!("invalid regex flag {:?}", c)),
            }
        }
        Ok(f)
    }

    pub fn to_string(&self) -> String {
        let mut s = String::new();
        if self.global { s.push('g'); }
        if self.ignore_case { s.push('i'); }
        if self.multiline { s.push('m'); }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_conversion() {
        assert_eq!(to_utf16("a中😀"), vec![0x0061, 0x4E2D, 0xD83D, 0xDE00]);
    }

    #[test]
    fn flags_parse() {
        let f = Flags::parse("gi").unwrap();
        assert!(f.global && f.ignore_case && !f.multiline);
        assert!(Flags::parse("x").is_err());
    }
}
