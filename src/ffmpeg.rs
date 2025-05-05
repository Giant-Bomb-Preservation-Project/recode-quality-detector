use std::process::Command;
use winnow::{ascii::*, combinator::*, prelude::*, stream::AsChar, token};

pub fn codecs() -> color_eyre::Result<Vec<Codec>> {
    Codec::parse_from_cli(std::str::from_utf8(
        &Command::new("ffmpeg").arg("-codecs").output()?.stdout,
    )?)
    .map_err(From::from)
}

pub fn is_available() -> bool {
    Command::new("ffmpeg").arg("--help").output().is_ok()
}

#[allow(dead_code)]
pub struct Codec {
    pub decodable: bool,
    pub encodable: bool,
    pub kind: CodecKind,
    pub intra_frame_only: bool,
    pub lossy_capable: bool,
    pub lossless_capable: bool,
    pub extension: String,
    pub name: String,
    pub encoders: Vec<String>,
    pub decoders: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum CodecKind {
    Video,
    Audio,
    Subtitle,
    Data,
    Attachment,
}

fn is_identifier(ch: char) -> bool {
    ch.is_alphanum() || ch == '_' || ch == '-' || ch == '.'
}

impl Codec {
    pub fn parse_from_cli(input: &str) -> color_eyre::Result<Vec<Codec>> {
        input
            .split_once("-------")
            .unwrap()
            .1
            .trim()
            .lines()
            .map(str::trim)
            .map(Codec::parse)
            .collect()
    }

    pub fn parse(ref mut input: &str) -> color_eyre::Result<Codec> {
        Self::parser()
            .parse(input)
            .map_err(|error| eyre::eyre!(error.to_string()))
    }

    pub fn parser<'input>() -> impl Parser<&'input str, Self, winnow::error::ContextError> {
        winnow::seq!(Codec {
            decodable: parse_flag("D"),
            encodable: parse_flag("E"),
            kind: parse_codec_kind,
            intra_frame_only: parse_flag("I"),
            lossy_capable: parse_flag("L"),
            lossless_capable: parse_flag("S"),
            _: multispace1,
            extension: token::take_while(1.., is_identifier).map(From::from),
            _: multispace1,
            name: repeat_till(1.., token::any, peek(alt((token::literal("(encoders:"), eof)))).map(|(name, _)| name),
            encoders: alt((parse_coders("encoders"), empty.default_value())),
            decoders: alt((parse_coders("decoders"), empty.default_value())),
        })
    }
}

fn parse_coders<'input>(
    coder: &'static str,
) -> impl Parser<&'input str, Vec<String>, winnow::error::ContextError> {
    winnow::seq!((
        _: "(",
        _: token::literal(coder),
        _: ":",
        _: multispace1,
        separated(1.., token::take_while(1.., is_identifier), space1).map(|coders: Vec<&'input str>| coders.into_iter().map(String::from).collect::<Vec<_>>()),
        _: ")",
    )).map(|(coders,)| coders)
}

fn parse_codec_kind(input: &mut &str) -> winnow::Result<CodecKind> {
    winnow::dispatch!(token::take(1usize);
        "V" => empty.value(CodecKind::Video),
        "A" => empty.value(CodecKind::Audio),
        "S" => empty.value(CodecKind::Subtitle),
        "D" => empty.value(CodecKind::Data),
        "T" => empty.value(CodecKind::Attachment),
        _ => fail::<_, CodecKind, _>,
    )
    .parse_next(input)
}

fn parse_flag(flag: &'_ str) -> impl Parser<&str, bool, winnow::error::ContextError> {
    alt((flag.value(true), ".".value(false)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse() {
        let input = "D.AIL. acelp.kelvin         Sipro ACELP.KELVIN";

        Codec::parse(input).unwrap();
    }
}
