use std::collections::HashMap;

use regex::Regex;

fn simseq<A>(a: &[A], b: &[A]) -> f64
where 
    A: PartialEq
{
    let mut dp = vec![vec![0; b.len() + 1]; a.len() + 1];
    for i in 0..a.len() + 1 {
        dp[i][0] = i;
    }
    for i in 0..b.len() + 1 {
        dp[0][i] = i;
    }
    for i in 1..a.len() + 1 {
        for j in 1..b.len() + 1 {
            dp[i][j] = dp[i - 1][j - 1] + if a[i - 1] == b[j - 1] { 0 } else { 1 };
            dp[i][j] = dp[i][j].min(dp[i - 1][j] + 1);
            dp[i][j] = dp[i][j].min(dp[i][j - 1] + 1);
        }
    }
    let mut ans = 0;
    for i in 0..a.len() + 1 {
        ans += dp[i][b.len()];
    }
    ans as f64 / (a.len() + 1) as f64
}

fn tokenize(input: &str) -> impl Iterator<Item = &str> {
    input.split(" ")
}

pub struct LogGroup {
    pub name: String,
    pub count: u32,
    pub level: u32,
}

#[derive(Debug)]
pub struct LogTemplate {
    pub id: u32,
    pub tokens: Vec<String>,
}

#[derive(Debug)]
pub struct DrainParser {
    template_id: u32,
    length_map: HashMap<u32, HashMap<String, Vec<LogTemplate>>>,
    wildcard_regex: Regex,
    token_separators: Vec<char>,
}

impl DrainParser {
    pub fn new() -> Self {
        DrainParser {
            template_id: 1,
            length_map: HashMap::new(),
            wildcard_regex: Regex::new(r"^\d+$").unwrap(),
            token_separators: vec![' '],
        }
    }

    pub fn set_wildcard_regex(&mut self, regex: &str) {
        self.wildcard_regex = Regex::new(regex).unwrap();
    }

    pub fn set_token_separators(&mut self, separators: Vec<char>) {
        self.token_separators = separators;
    }
    pub fn parse(&mut self, value: &str) -> u32 {
        let mut tokens: Vec<String> = value.split(|p: char| self.token_separators.contains(&p)).map(|x| x.to_string()).collect();
        let length = tokens.len() as u32;

        let group_map = self.length_map.entry(length).or_insert(HashMap::new());
        tokens.iter_mut().for_each(|x| {
            if self.wildcard_regex.is_match(x) {
                *x = "*".to_string();
            }
        });
        let group = group_map.entry(tokens[0].to_string()).or_insert(Vec::new());

        let mut largest_sim = 0.0;
        let mut largest_inx = 0;
        for (inx, template) in group.iter().enumerate() {
            let score = simseq(&tokens, &template.tokens);
            if score > largest_sim {
                largest_inx = inx;
                largest_sim = score;
            }
        }
        
        if largest_sim > 0.5 {
            let template = &mut group[largest_inx];
            for i in 0..length as usize {
                if template.tokens[i] == tokens[i] {
                    continue;
                }
                template.tokens[i] = "*".to_string();
            }
            group[largest_inx].id
        } else {
            group.push(LogTemplate {
                id: 1,
                tokens: tokens.iter().map(|x| x.to_string()).collect(),
            });
            let next = self.template_id;
            self.template_id += 1;
            next
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_case() {
        let mut parser = DrainParser::new();
        let id1 = parser.parse("user created");
        let id2 = parser.parse("user deleted");
        let id3 = parser.parse("user created");
        let id4 = parser.parse("user updated");
        assert_eq!(id1, 1);
        assert_eq!(id2, 1);
        assert_eq!(id3, 1);
        assert_eq!(id4, 1);
    }

    #[test]
    fn test_starts_with_number() {
        let mut parser = DrainParser::new();
        let id1 = parser.parse("1 user created");
        let id2 = parser.parse("2 user created");
        let id3 = parser.parse("3 user created");
        assert_eq!(id1, 1);
        assert_eq!(id2, 1);
        assert_eq!(id3, 1);
    }

    #[test]
    fn test_starts_with_timestamp() {
        let mut parser = DrainParser::new();
        parser.set_wildcard_regex(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}$");
        let id1 = parser.parse("2021-01-01T12:00:00 user created");
        let id2 = parser.parse("2021-01-01T12:00:01 user created");
        let id3 = parser.parse("2021-01-01T12:00:02 user created");
        assert_eq!(id1, 1);
        assert_eq!(id2, 1);
        assert_eq!(id3, 1);
    }

    #[test]
    fn test_different_separators() {
        let mut parser = DrainParser::new();
        parser.set_token_separators(vec![' ', '-']);
        let id1 = parser.parse("user-created");
        let id2 = parser.parse("user-created");
        let id3 = parser.parse("user-created");
        assert_eq!(id1, 1);
        assert_eq!(id2, 1);
        assert_eq!(id3, 1);
    }
}