const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec"
];

function startOfDay(date: Date): number {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

export type TimeFormat = "system" | "12h" | "24h";

function clockOptions(format: TimeFormat): { hour12?: boolean } {
  return format === "system" ? {} : { hour12: format === "12h" };
}

export function formatTime(date: Date, format: TimeFormat = "system"): string {
  return new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    ...clockOptions(format)
  }).format(date);
}

export function relativeDate(
  timestampSeconds: number,
  nowMs = Date.now(),
  format: TimeFormat = "system"
): string {
  if (!timestampSeconds) return "";
  const date = new Date(timestampSeconds * 1000);
  if (Number.isNaN(date.getTime())) return "";
  const now = new Date(nowMs);
  const days = Math.round((startOfDay(now) - startOfDay(date)) / 86_400_000);

  if (days <= 0) return formatTime(date, format);
  if (days === 1) return "Yesterday";
  if (days < 7) return WEEKDAYS[date.getDay()];
  if (date.getFullYear() === now.getFullYear()) {
    return `${MONTHS[date.getMonth()]} ${date.getDate()}`;
  }
  return `${MONTHS[date.getMonth()]} ${date.getDate()}, ${date.getFullYear()}`;
}

export function formatFullDate(
  timestampSeconds: number,
  format: TimeFormat = "system"
): string {
  if (!timestampSeconds) return "";
  const date = new Date(timestampSeconds * 1000);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleString(undefined, {
    weekday: "short",
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    ...clockOptions(format)
  });
}

export function initials(name: string, email: string): string {
  const words = name.trim().split(/\s+/).filter(Boolean);
  if (words.length > 0) {
    return words
      .slice(0, 2)
      .map((word) => word[0]?.toUpperCase() ?? "")
      .join("");
  }
  return email.trim().charAt(0).toUpperCase() || "?";
}
