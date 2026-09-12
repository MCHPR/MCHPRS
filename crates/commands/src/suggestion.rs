use crate::{Argument, Graph, NodeId, Reader, Span};

#[derive(Debug)]
pub struct SuggestionTarget {
    pub node: NodeId,
    pub start: usize,
    pub path: Vec<NodeId>,
}

impl<A: Argument, E, M> Graph<A, E, M> {
    pub fn suggestion_targets(
        &self,
        input: &str,
        allowed: impl Fn(&M) -> bool,
    ) -> Vec<SuggestionTarget> {
        let mut targets = Vec::new();
        let mut pending = vec![(self.root(), Reader::new(input), Vec::new())];
        while let Some((parent, reader, path)) = pending.pop() {
            if path.len() > 256 {
                continue;
            }
            let literal = self.matching_literal(parent, reader.remaining());
            for &id in self.node(parent).children() {
                if reader.remaining().contains(' ') && literal.is_some_and(|literal| literal != id)
                {
                    continue;
                }
                let node = self.node(id);
                if !allowed(&node.metadata) {
                    continue;
                }
                let mut next = reader;
                let parsed = node.kind.parse(&mut next).is_ok();
                if !parsed || next.peek() != Some(' ') {
                    targets.push(SuggestionTarget {
                        node: id,
                        start: reader.cursor(),
                        path: path.clone(),
                    });
                    continue;
                }
                next.read();
                let mut path = path.clone();
                path.push(id);
                if let Some(destination) = node.redirect() {
                    pending.push((destination, next, path));
                    break;
                }
                pending.push((id, next, path));
            }
        }
        targets
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub span: Span,
    pub text: String,
    pub tooltip: Option<String>,
}

pub struct SuggestionsBuilder<'input, 'output> {
    input: &'input str,
    span: Span,
    suggestions: &'output mut Vec<Suggestion>,
}

impl<'input, 'output> SuggestionsBuilder<'input, 'output> {
    pub fn new(input: &'input str, span: Span, suggestions: &'output mut Vec<Suggestion>) -> Self {
        assert!(
            input.get(span.start..span.end).is_some(),
            "Suggestion ranges must follow UTF-8 boundaries"
        );
        Self {
            input,
            span,
            suggestions,
        }
    }

    pub fn input(&self) -> &'input str {
        self.input
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn remaining(&self) -> &'input str {
        &self.input[self.span.start..self.span.end]
    }

    pub fn at(&mut self, start: usize) -> SuggestionsBuilder<'input, '_> {
        assert!(
            start >= self.span.start,
            "Suggestion offset precedes its parent"
        );
        SuggestionsBuilder::new(
            self.input,
            Span::new(start, self.span.end),
            self.suggestions,
        )
    }

    pub fn matches(&self, value: &str) -> bool {
        let prefix = self.remaining();
        if value.is_ascii() && prefix.is_ascii() {
            value
                .get(..prefix.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        } else {
            value.to_lowercase().starts_with(&prefix.to_lowercase())
        }
    }

    pub fn suggest(&mut self, text: impl Into<String>) {
        self.suggestions.push(Suggestion {
            span: self.span,
            text: text.into(),
            tooltip: None,
        });
    }

    pub fn suggest_with_tooltip(&mut self, text: impl Into<String>, tooltip: impl Into<String>) {
        self.suggestions.push(Suggestion {
            span: self.span,
            text: text.into(),
            tooltip: Some(tooltip.into()),
        });
    }

    pub fn suggest_matching(&mut self, values: impl IntoIterator<Item = impl AsRef<str>>) {
        for value in values {
            if self.matches(value.as_ref()) {
                self.suggest(value.as_ref());
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub text: String,
    pub tooltip: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Suggestions {
    pub span: Span,
    pub completions: Vec<Completion>,
}

impl Suggestions {
    pub fn merge(input: &str, suggestions: Vec<Suggestion>) -> Self {
        let Some(first) = suggestions.first() else {
            return Self {
                span: Span::new(input.len(), input.len()),
                completions: Vec::new(),
            };
        };
        let span = suggestions.iter().fold(first.span, |span, suggestion| {
            Span::new(
                span.start.min(suggestion.span.start),
                span.end.max(suggestion.span.end),
            )
        });
        let mut completions: Vec<_> = suggestions
            .into_iter()
            .map(|suggestion| Completion {
                text: format!(
                    "{}{}{}",
                    &input[span.start..suggestion.span.start],
                    suggestion.text,
                    &input[suggestion.span.end..span.end]
                ),
                tooltip: suggestion.tooltip,
            })
            .collect();
        completions.sort_by(|left, right| {
            left.text
                .cmp(&right.text)
                .then(left.tooltip.is_none().cmp(&right.tooltip.is_none()))
        });
        completions.dedup_by(|later, earlier| later.text == earlier.text);
        Self { span, completions }
    }
}
