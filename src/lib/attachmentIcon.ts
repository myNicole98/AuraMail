export type AttachmentKind =
  | "pdf"
  | "image"
  | "archive"
  | "spreadsheet"
  | "audio"
  | "video"
  | "code"
  | "text"
  | "file";

const CODE_EXTENSIONS = new Set([
  "json",
  "xml",
  "js",
  "ts",
  "html",
  "css",
  "rs",
  "py",
  "sh",
  "toml",
  "yaml",
  "yml"
]);
const TEXT_EXTENSIONS = new Set(["doc", "docx", "odt", "rtf", "txt", "md", "markdown"]);
const ARCHIVE_EXTENSIONS = new Set(["zip", "rar", "7z", "tar", "gz", "bz2", "xz"]);
const SPREADSHEET_EXTENSIONS = new Set(["xls", "xlsx", "ods", "csv"]);

export function attachmentKind(mimeType: string, filename: string): AttachmentKind {
  const mime = mimeType.toLowerCase();
  const extension = filename.toLowerCase().match(/\.([a-z0-9]+)$/)?.[1] ?? "";

  if (mime === "application/pdf" || extension === "pdf") return "pdf";
  if (mime.startsWith("image/")) return "image";
  if (
    /zip|rar|7z|tar|gzip|compressed|x-bzip/.test(mime) ||
    ARCHIVE_EXTENSIONS.has(extension)
  ) {
    return "archive";
  }
  if (
    /spreadsheet|excel|csv/.test(mime) ||
    SPREADSHEET_EXTENSIONS.has(extension)
  ) {
    return "spreadsheet";
  }
  if (mime.startsWith("audio/")) return "audio";
  if (mime.startsWith("video/")) return "video";
  if (
    /json|xml|javascript|typescript|html|css|rust|python/.test(mime) ||
    CODE_EXTENSIONS.has(extension)
  ) {
    return "code";
  }
  if (mime.startsWith("text/") || TEXT_EXTENSIONS.has(extension)) return "text";
  return "file";
}
