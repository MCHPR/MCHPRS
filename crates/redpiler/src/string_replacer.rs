use std::borrow::Cow;

pub struct StringReplacer<'a> {
    src: &'a str,
    read: usize,
    output: String,
}

impl<'a> StringReplacer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            read: 0,
            output: String::new(),
        }
    }

    pub fn replace_range(&mut self, range: std::ops::Range<usize>, with: &str) {
        assert!(
            range.start >= self.read,
            "Replacements should be done in order {} < {}",
            range.start,
            self.read,
        );
        self.output += &self.src[self.read..range.start];
        self.output += with;
        self.read = range.end;
    }

    pub fn updated(&self) -> bool {
        !self.output.is_empty() || self.read > 0
    }

    pub fn finish(self) -> Cow<'a, str> {
        if self.updated() {
            Cow::Owned(self.output + &self.src[self.read..])
        } else {
            Cow::Borrowed(self.src)
        }
    }
}

#[test]
fn test() {
    let mut sr = StringReplacer::new("hello there");

    let hello = sr.src.find("hello").unwrap();
    let her = sr.src.find("her").unwrap();

    sr.replace_range(hello..hello + 5, "bye");
    sr.replace_range(her..her + 3, "elephon");

    assert_eq!(sr.finish().as_ref(), "bye telephone");
}
