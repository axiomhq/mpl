use std::fs;

use mpl_lang::lexer::Token;

#[test]
fn lex_examples() {
    fs::read_dir("./tests/examples")
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "mpl"))
        .for_each(|entry| {
            let path = entry.path();
            let file_name = path.file_name().unwrap().to_str().unwrap();
            println!("Running example: {file_name}");
            let content = fs::read_to_string(&path).unwrap();
            let mut lexer = mpl_lang::lexer::Lexer::new(&content);
            if let Some(Token::Invalid(pos, t)) = lexer.find(|t| matches!(t, Token::Invalid(_, _)))
            {
                panic!("[{file_name}] Invalid token at position {pos}: `{t}`");
            }
        });
}

#[test]
fn parse_unimplemented_examples() {
    fs::read_dir("./tests/examples")
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|ext| ext == "mpl-todo")
        })
        .for_each(|entry| {
            let path = entry.path();
            let file_name = path.file_name().unwrap().to_str().unwrap();
            println!("Running example: {file_name}");
            let content = fs::read_to_string(&path).unwrap();
            let mut lexer = mpl_lang::lexer::Lexer::new(&content);
            if let Some(Token::Invalid(pos, t)) = lexer.find(|t| matches!(t, Token::Invalid(_, _)))
            {
                panic!("[{file_name}] Invalid token at position {pos}: `{t}`");
            }
        });
}

#[test]
fn lex_error_examples() {
    let failing = ["invalid_operator.mpl"];
    fs::read_dir("./tests/errors")
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "mpl"))
        .for_each(|entry| {
            let path = entry.path();
            let file_name = path.file_name().unwrap().to_str().unwrap();
            println!("Running error case: {file_name}");
            let content = fs::read_to_string(&path).unwrap();
            let mut lexer = mpl_lang::lexer::Lexer::new(&content);
            if failing.contains(&file_name) {
                assert!(
                    lexer.find(|t| matches!(t, Token::Invalid(_, _))).is_some(),
                    "[{file_name}] should fail"
                );
            } else if let Some(Token::Invalid(pos, t)) =
                lexer.find(|t| matches!(t, Token::Invalid(_, _)))
            {
                panic!("[{file_name}] Invalid token at position {pos}: `{t}`");
            }
        });
}
