#[derive(Debug, Eq, PartialEq)]
pub enum Shell {
    Bash,
    Zsh,
}

impl Shell {
    pub const fn script(&self) -> &'static str {
        match self {
            Self::Bash => include_str!("../completions/ghist.bash"),
            Self::Zsh => include_str!("../completions/ghist.zsh"),
        }
    }
}
