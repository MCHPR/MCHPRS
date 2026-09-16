use super::{argument::ClientArgument, node::Policy, registry::CommandRegistry};
use crate::player::Player;
use mchprs_commands::{NodeKind, Span, Suggestion, Suggestions, SuggestionsBuilder};
use mchprs_network::packets::clientbound::{
    CCommandSuggestionsResponse, CCommandSuggestionsResponseMatch,
};
use mchprs_text::TextComponentBuilder;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SuggestionSource {
    PlayerNames,
    SchematicFiles,
}

#[derive(Debug)]
pub struct CommandSuggestions {
    text: String,
    transaction_id: i32,
    suggestions: Vec<Suggestion>,
    requests: Vec<SuggestionRequest>,
}

#[derive(Debug)]
struct SuggestionRequest {
    source: SuggestionSource,
    span: Span,
}

impl CommandRegistry {
    pub(crate) fn suggestions(
        &self,
        text: &str,
        transaction_id: i32,
        player: &Player,
    ) -> CommandSuggestions {
        let mut result = CommandSuggestions {
            text: text.to_owned(),
            transaction_id,
            suggestions: Vec::new(),
            requests: Vec::new(),
        };
        let Some(input) = text.strip_prefix('/') else {
            return result;
        };
        let allowed = |policy: &Policy| policy.allows(player);
        let Some((command, _)) = input.split_once(' ') else {
            let mut builder =
                SuggestionsBuilder::new(input, Span::new(0, input.len()), &mut result.suggestions);
            builder.suggest_matching(self.visible_commands(&allowed).map(|(name, _)| name));
            return result;
        };

        let custom_alias = self.aliases.contains_key(command);
        let expansion = self.expand(input);
        let cursor = expansion
            .expanded_cursor(input.len())
            .unwrap_or(expansion.text.len());
        let prefix = &expansion.text[..cursor];
        for target in self.graph.suggestion_targets(prefix, allowed) {
            let node = self.graph.node(target.node);
            let server_only = custom_alias
                || target.path.iter().any(|&id| {
                    matches!(&self.graph.node(id).kind, NodeKind::Argument { parser, .. }
                    if matches!(parser.client_argument(), ClientArgument::Opaque))
                });
            let span = Span::new(target.start, cursor);
            let mut candidates = Vec::new();
            let mut builder = SuggestionsBuilder::new(prefix, span, &mut candidates);
            match &node.kind {
                NodeKind::Literal(name) => builder.suggest_matching([name]),
                NodeKind::Argument { parser, .. } => {
                    if !server_only && matches!(parser.client_argument(), ClientArgument::Client(_))
                    {
                        continue;
                    }
                    if let Some(source) = parser.suggest(&mut builder)
                        && let Some(span) = expansion.original_span(span)
                    {
                        result.requests.push(SuggestionRequest { source, span });
                    }
                }
                NodeKind::Root => {}
            }
            result
                .suggestions
                .extend(candidates.into_iter().filter_map(|suggestion| {
                    expansion
                        .original_span(suggestion.span)
                        .map(|span| Suggestion { span, ..suggestion })
                }));
        }
        result
    }
}

impl CommandSuggestions {
    pub(crate) fn needs(&self, source: SuggestionSource) -> bool {
        self.requests.iter().any(|request| request.source == source)
    }

    pub(crate) fn resolve(
        &mut self,
        source: SuggestionSource,
        values: impl IntoIterator<Item = impl AsRef<str>>,
    ) {
        let Some(input) = self.text.strip_prefix('/') else {
            return;
        };
        let values: Vec<_> = values.into_iter().collect();
        self.requests.retain(|request| {
            if request.source != source {
                return true;
            }
            SuggestionsBuilder::new(input, request.span, &mut self.suggestions)
                .suggest_matching(&values);
            false
        });
    }

    pub(crate) fn into_response(self) -> CCommandSuggestionsResponse {
        assert!(
            self.requests.is_empty(),
            "Suggestion sources must be resolved before encoding"
        );
        let Some(input) = self.text.strip_prefix('/') else {
            return CCommandSuggestionsResponse {
                id: self.transaction_id,
                start: self.text.encode_utf16().count() as i32,
                length: 0,
                matches: Vec::new(),
            };
        };
        let suggestions = Suggestions::merge(input, self.suggestions);
        let span = Span::new(suggestions.span.start + 1, suggestions.span.end + 1);
        let (start, length) = span
            .utf16(&self.text)
            .expect("Suggestion ranges follow UTF-8 boundaries");
        CCommandSuggestionsResponse {
            id: self.transaction_id,
            start: start as i32,
            length: length as i32,
            matches: suggestions
                .completions
                .into_iter()
                .take(1000)
                .map(|completion| CCommandSuggestionsResponseMatch {
                    match_: completion.text,
                    tooltip: completion
                        .tooltip
                        .map(|tooltip| TextComponentBuilder::new(tooltip).finish()),
                })
                .collect(),
        }
    }
}
