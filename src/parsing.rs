
// pub fn parse_logline(logline: &str) -> Option<LogLine> {
//     let re = Regex::new(r"^(?P<timestamp>\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}) (?P<level>\w+): (?P<message>.*)$").unwrap();
//     let captures = re.captures(logline)?;

//     Some(LogLine {
//         timestamp: captures.name("timestamp")?.as_str().to_string(),
//         level: captures.name("level")?.as_str().to_string(),
//         message: captures.name("message")?.as_str().to_string(),
//     })
// }