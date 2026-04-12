use std::fs;

#[test]
fn parse_examples() {
    fs::read_dir("./tests/examples")
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "mpl"))
        .for_each(|entry| {
            let path = entry.path();
            let file_name = path.file_name().unwrap().to_str().unwrap();
            println!("Running example: {file_name}");
            let content = fs::read_to_string(path).unwrap();
            match mpl_lang::compile(&content) {
                Ok(_) => println!("Parsed successfully"),
                Err(mpl_lang::CompileError::Parse(mpl_lang::ParseError::NotSupported {
                    span,
                    rule,
                })) => {
                    println!("Parsed but not supported by the backend: {span:?}, {rule:?}")
                }
                Err(mpl_lang::CompileError::Parse(mpl_lang::ParseError::NotImplemented(
                    feature,
                ))) => {
                    println!("Parsed but not yet implemented: {feature}")
                }
                Err(e) => panic!("Error parsing {:?}: {e} {e:?}", entry.file_name()),
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
            let content = fs::read_to_string(path).unwrap();
            match mpl_lang::compile(&content) {
                Ok(_) => panic!("Unexpected successfully parsing"),
                Err(mpl_lang::CompileError::Parse(mpl_lang::ParseError::NotSupported {
                    span,
                    rule,
                })) => {
                    panic!("Unexpected parse but unsupported: {span:?}, {rule:?}")
                }
                Err(_) => println!("Failing as expected."),
            }
        });
}

#[test]
fn parse_error_examples() {
    fs::read_dir("./tests/errors")
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "mpl"))
        .for_each(|entry| {
            let path = entry.path();
            let file_name = path.file_name().unwrap().to_str().unwrap();
            println!("Running error case: {file_name}");
            let content = fs::read_to_string(path).unwrap();
            match mpl_lang::compile(&content) {
                Ok(_) => panic!("Unexpected successfully parsing"),
                Err(_) => println!("Failing as expected."),
            }
        });
}
