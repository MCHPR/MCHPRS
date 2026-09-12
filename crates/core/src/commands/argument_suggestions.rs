use super::{argument::FlagSpec, argument_parser::parse_flags};
use mchprs_commands::{Argument, Reader, Span, SuggestionsBuilder};
use std::iter;

pub(super) fn flags(flags: &[FlagSpec], builder: &mut SuggestionsBuilder<'_, '_>) {
    let typed = builder.remaining();
    let token_start = typed.rfind(' ').map_or(0, |index| index + 1);
    let mut reader = Reader::new(&typed[..token_start]);
    let Ok(used) = parse_flags(&mut reader, flags) else {
        return;
    };
    let offset = builder.span().start + token_start;
    let mut token = builder.at(offset);
    for flag in flags
        .iter()
        .filter(|flag| !used.contains(flag.long.as_str()))
    {
        let names = iter::once(format!("--{}", flag.long))
            .chain(flag.short.map(|short| format!("-{short}")));
        for name in names {
            if token.matches(&name) {
                token.suggest_with_tooltip(name, &flag.description);
            }
        }
    }
}

pub(super) fn coordinates(
    parser: &impl Argument,
    templates: &[&str],
    builder: &mut SuggestionsBuilder<'_, '_>,
) {
    let entered: Vec<_> = builder.remaining().split(' ').collect();
    for template in templates {
        let mut coordinates: Vec<_> = template.split(' ').collect();
        if entered.len() > coordinates.len() {
            continue;
        }
        for (coordinate, entered) in coordinates.iter_mut().zip(&entered) {
            if !entered.is_empty() {
                *coordinate = entered;
            }
        }
        let suggestion = coordinates.join(" ");
        let mut reader = Reader::new(&suggestion);
        if parser.parse(&mut reader).is_ok() && !reader.can_read() {
            builder.suggest_matching([suggestion]);
        }
    }
}

pub(super) fn quoted(
    parser: &impl Argument,
    builder: &mut SuggestionsBuilder<'_, '_>,
    suggest: impl FnOnce(&mut SuggestionsBuilder<'_, '_>),
) {
    let Some(quote @ ('\'' | '"')) = builder.remaining().chars().next() else {
        suggest(builder);
        return;
    };

    let mut reader = Reader::new(builder.remaining());
    let closed = reader.quoted().is_ok();
    if closed && reader.can_read() {
        return;
    }
    let input = builder.input();
    let span = builder.span();
    let end = span.end - usize::from(closed);
    let mut suggestions = Vec::new();
    suggest(&mut SuggestionsBuilder::new(
        input,
        Span::new(span.start + 1, end),
        &mut suggestions,
    ));
    for mut suggestion in suggestions {
        let completed = format!(
            "{}{}{quote}",
            &input[span.start..suggestion.span.start],
            suggestion.text
        );
        let mut reader = Reader::new(&completed);
        if parser.parse(&mut reader).is_ok() && !reader.can_read() {
            suggestion.text.push(quote);
        }
        builder.at(suggestion.span.start).suggest(suggestion.text);
    }
}
