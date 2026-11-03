import type { ContactRecord } from "./types";

export function contactInitials(name: string, email: string): string {
  const source = name.trim() || email.trim();
  if (!source) return "?";
  const words = source.split(/\s+/).filter(Boolean);
  if (words.length >= 2) {
    return `${words[0][0]}${words[words.length - 1][0]}`.toUpperCase();
  }
  return source.slice(0, 2).toUpperCase();
}

export function contactTitle(record: ContactRecord): string {
  const name = record.displayName.trim();
  if (name) return name;
  const email = record.emails.find((value) => value.trim());
  if (email) return email.trim();
  const phone = record.phones.find((value) => value.trim());
  if (phone) return phone.trim();
  return "Unnamed contact";
}

export function validEmails(emails: string[]): boolean {
  return emails.every((email) => {
    const value = email.trim();
    return !value || value.includes("@");
  });
}

export function contactMatches(record: ContactRecord, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (!needle) return true;
  return (
    record.displayName.toLowerCase().includes(needle) ||
    record.emails.some((email) => email.toLowerCase().includes(needle))
  );
}
