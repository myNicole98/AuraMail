export const SPACE_ICON_NAMES = [
  "layers",
  "user",
  "briefcase",
  "folder",
  "inbox",
  "users",
  "star",
  "sparkles",
  "heart",
  "plane",
  "book",
  "code"
] as const;

export type SpaceIconName = (typeof SPACE_ICON_NAMES)[number];
