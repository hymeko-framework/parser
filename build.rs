fn main() -> Result<(), Box<dyn std::error::Error>> {
    lalrpop::process_root()?;
    let source = std::fs::read_to_string("src/hymeko.lalrpop")?;
    let mut fast = String::new();
    for line in source.lines() {
        if let Some((left, token)) = line.split_once(" => crate::lexer::Token::") {
            fast.push_str(&format!(
                "{left} => crate::fusion::Token::Plain(crate::lexer::Token::{}),\n",
                token.trim_end_matches(',')
            ));
        } else {
            fast.push_str(line);
            fast.push('\n');
        }
    }
    fast = fast.replace("use crate::lexer::Token;", "use crate::fusion::Token;");
    let terminal = "    enum Token<'a> {";
    assert!(fast.contains(terminal));
    fast = fast.replace(terminal, &format!("{terminal}\n        FusedArc => crate::fusion::Token::Arc(<Vec<crate::builder::LiteralReference<'a>>>),"));
    let rule = "HyperArc: B::Item = {";
    assert!(fast.contains(rule));
    fast = fast.replace(
        rule,
        &format!("{rule}\n    <refs:FusedArc> => builder.literal_arc(refs),"),
    );
    let path = std::path::PathBuf::from(std::env::var_os("OUT_DIR").ok_or("missing OUT_DIR")?)
        .join("fast.lalrpop");
    std::fs::write(&path, fast)?;
    lalrpop::Configuration::new().process_file(path)?;
    Ok(())
}
