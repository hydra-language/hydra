use lexer::{Lexer, NameInterner};

#[test]
fn identifier_ids_are_shared_across_lexer_instances() {
    let mut names = NameInterner::default();

    let first = {
        let mut lexer = Lexer::new("value::push".into(), &mut names);
        lexer.tokenize().unwrap()
    };

    let second = {
        let mut lexer = Lexer::new("other::push".into(), &mut names);
        lexer.tokenize().unwrap()
    };

    assert_eq!(first[2].name_id, second[2].name_id);
    assert_ne!(first[0].name_id, second[0].name_id);
}
