//! Email parsing contract. Zero-dep.

use core::future::Future;

pub trait MailParser: Send + Sync {
    type Doc;
    type Error: core::fmt::Debug + Send + Sync + 'static;

    fn parse<'a>(
        &'a self,
        raw: &'a [u8],
        hints: ParseHints<'a>,
    ) -> impl Future<Output = Result<Self::Doc, Self::Error>> + Send + 'a;
}

pub struct ParseHints<'a> {
    /// MIME boundary marker if already known from the envelope.
    pub boundary: Option<&'a [u8]>,
    /// Preferred content type order for rendering.
    pub preferred_types: &'a [&'a str],
    /// Maximum bytes to parse; implementations MUST refuse larger.
    pub max_bytes: usize,
    /// Language codes (BCP-47) that the parser should prioritize
    /// when running AI extraction.
    pub locales: &'a [&'a str],
}

/// A MIME part location within a parsed mail, opaque to the
/// contract. Consumers get this from their `MailParser::Doc`.
pub struct PartRef(pub u32);

pub struct AttachmentRef<'a> {
    pub filename: Option<&'a str>,
    pub content_type: &'a str,
    pub size: u64,
    pub inline: bool,
}

pub trait ThreadLinker: Send + Sync {
    /// Given `message-id` + `in-reply-to` + `references`, return a
    /// stable thread key. Implementations may hash, bucket, or
    /// persist; the contract only requires determinism per instance.
    fn thread_key(
        &self,
        message_id: &str,
        in_reply_to: Option<&str>,
        references: &[&str],
    ) -> [u8; 16];
}
