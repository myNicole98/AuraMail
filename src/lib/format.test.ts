import { describe, expect, it } from "vitest";
import { formatFullDate, initials, relativeDate } from "./format";

const now = new Date(2026, 9, 18, 12, 0, 0).getTime();
const seconds = (date: Date) => Math.floor(date.getTime() / 1000);

describe("relativeDate", () => {
  it("shows the time for messages from today", () => {
    const today = new Date(2026, 9, 18, 10, 24, 0);
    expect(relativeDate(seconds(today), now, "24h")).toBe("10:24");
  });

  it("formats today's time as 24-hour when requested", () => {
    const today = new Date(2026, 9, 18, 0, 5, 0);
    expect(relativeDate(seconds(today), now, "24h")).toBe("00:05");
  });

  it("formats today's time as 12-hour when requested", () => {
    const today = new Date(2026, 9, 18, 10, 24, 0);
    expect(relativeDate(seconds(today), now, "12h")).toBe("10:24 AM");
  });

  it("shows Yesterday for the previous day", () => {
    const yesterday = new Date(2026, 9, 17, 23, 0, 0);
    expect(relativeDate(seconds(yesterday), now)).toBe("Yesterday");
  });

  it("shows a weekday within the last week", () => {
    const monday = new Date(2026, 9, 12, 9, 0, 0);
    expect(relativeDate(seconds(monday), now)).toBe("Mon");
  });

  it("shows a short date for older messages", () => {
    const older = new Date(2026, 8, 2, 9, 0, 0);
    expect(relativeDate(seconds(older), now)).toBe("Sep 2");
  });

  it("returns an empty string without a valid timestamp", () => {
    expect(relativeDate(0, now)).toBe("");
  });
});

describe("formatFullDate", () => {
  it("formats a full date with the requested clock", () => {
    const date = new Date(2026, 9, 18, 10, 24, 0);
    expect(formatFullDate(seconds(date), "12h")).toContain("10:24 AM");
    expect(formatFullDate(seconds(date), "24h")).toContain("10:24");
    expect(formatFullDate(seconds(date), "24h")).not.toContain("AM");
  });
});

describe("initials", () => {
  it("uses the sender name", () => {
    expect(initials("Mara Klein", "mara@example.com")).toBe("MK");
  });

  it("falls back to the email address", () => {
    expect(initials("", "ada@example.com")).toBe("A");
  });
});
