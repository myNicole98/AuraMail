<script lang="ts">
  import { onMount, tick } from "svelte";
  import {
    CalendarDays,
    ChevronLeft,
    ChevronRight,
    Clock,
    Eye,
    EyeOff,
    LoaderCircle,
    Lock,
    MapPin,
    Pencil,
    Plus,
    RefreshCw,
    Repeat,
    Trash2,
    X
  } from "lucide-svelte";
  import { api } from "../api";
  import { app } from "../stores/app.svelte";
  import { appearance } from "../stores/appearance.svelte";
  import type { CalendarAccount, CalendarEvent, CalendarSummary } from "../types";
  import CalendarAccountDialog from "./calendar/CalendarAccountDialog.svelte";

  type CalendarView = "day" | "week" | "multiweek" | "month";
  type Day = {
    date: Date;
    key: string;
    inMonth: boolean;
  };
  type EventDraft = {
    summary: string;
    date: string;
    start: string;
    end: string;
    allDay: boolean;
    location: string;
    description: string;
    recurrence: string;
  };
  type GridMenu =
    | {
        x: number;
        y: number;
        date: Date;
        hour: number;
        minute: number;
        event?: undefined;
      }
    | {
        x: number;
        y: number;
        event: CalendarEvent;
        date?: undefined;
        hour?: undefined;
        minute?: undefined;
      };
  type PopoverPosition = { left: number; top: number };

  const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
  const WEEKDAYS_SHORT = ["S", "M", "T", "W", "T", "F", "S"];
  const HOURS = Array.from({ length: 24 }, (_, hour) => hour);
  const HOUR_HEIGHT = 56;
  const DATE_INPUT_ORDER = new Intl.DateTimeFormat(undefined, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit"
  })
    .formatToParts(new Date(2000, 10, 28))
    .filter((part) => part.type === "year" || part.type === "month" || part.type === "day")
    .map((part) => part.type);
  const REPEAT_OPTIONS = [
    { value: "", label: "Does not repeat" },
    { value: "FREQ=DAILY", label: "Daily" },
    { value: "FREQ=WEEKLY", label: "Weekly" },
    {
      value: "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR",
      label: "Every weekday"
    },
    { value: "FREQ=MONTHLY", label: "Monthly" },
    { value: "FREQ=YEARLY", label: "Yearly" }
  ];

  let calendars = $state<CalendarSummary[]>([]);
  let events = $state<CalendarEvent[]>([]);
  let hiddenCalendars = $state<Set<number>>(new Set());
  let loading = $state(true);
  let loadedSpaceId = $state<number | null>(null);
  let syncing = $state(false);
  let creating = $state(false);
  let deletingId = $state<number | null>(null);
  let error = $state<string | null>(null);
  let createError = $state<string | null>(null);
  let showAddAccount = $state(false);
  let showCreate = $state(false);
  let editingEventId = $state<number | null>(null);
  let selectedCalendarId = $state<number | null>(null);
  let selectedEvent = $state<CalendarEvent | null>(null);
  let eventPopover = $state<PopoverPosition | null>(null);
  let gridMenu = $state<GridMenu | null>(null);
  let selectedSlot = $state<{ dateKey: string; minutes: number } | null>(null);
  let calendarMenu = $state<{
    x: number;
    y: number;
    calendar: CalendarSummary;
  } | null>(null);
  let view = $state<CalendarView>("week");
  let anchor = $state(startOfDay(new Date()));
  let miniCursor = $state(
    new Date(new Date().getFullYear(), new Date().getMonth(), 1)
  );
  let timeScroll = $state<HTMLDivElement | null>(null);
  let rangeRequest = 0;
  let draftEvent = $state<EventDraft>(newDraft(startOfDay(new Date())));
  let dateInput = $state("");
  let startInput = $state("");
  let endInput = $state("");
  let showDatePicker = $state(false);
  let datePickerPosition = $state<PopoverPosition | null>(null);
  let datePickerCursor = $state(startOfMonth(new Date()));
  let datePickerDays = $derived(
    daysFor(startOfWeek(startOfMonth(datePickerCursor)), 42, datePickerCursor)
  );

  function startOfDay(date: Date): Date {
    return new Date(date.getFullYear(), date.getMonth(), date.getDate());
  }

  function dayKey(date: Date): string {
    const year = date.getFullYear();
    const month = String(date.getMonth() + 1).padStart(2, "0");
    const day = String(date.getDate()).padStart(2, "0");
    return `${year}-${month}-${day}`;
  }

  function parseDayKey(key: string): Date {
    const [year, month, day] = key.split("-").map(Number);
    return new Date(year, month - 1, day);
  }

  function utcDayKey(date: Date): string {
    const year = date.getUTCFullYear();
    const month = String(date.getUTCMonth() + 1).padStart(2, "0");
    const day = String(date.getUTCDate()).padStart(2, "0");
    return `${year}-${month}-${day}`;
  }

  function addDays(date: Date, amount: number): Date {
    const next = new Date(date);
    next.setDate(next.getDate() + amount);
    return next;
  }

  function startOfWeek(date: Date): Date {
    const start = startOfDay(date);
    return addDays(start, -start.getDay());
  }

  function startOfMonth(date: Date): Date {
    return new Date(date.getFullYear(), date.getMonth(), 1);
  }

  function weekNumber(date: Date): number {
    const value = new Date(
      Date.UTC(date.getFullYear(), date.getMonth(), date.getDate())
    );
    value.setUTCDate(value.getUTCDate() + 4 - (value.getUTCDay() || 7));
    const yearStart = new Date(Date.UTC(value.getUTCFullYear(), 0, 1));
    return Math.ceil(
      ((value.getTime() - yearStart.getTime()) / 86_400_000 + 1) / 7
    );
  }

  function rangeFor(selectedView: CalendarView, selectedAnchor: Date): {
    start: Date;
    days: number;
  } {
    if (selectedView === "day") {
      return { start: startOfDay(selectedAnchor), days: 1 };
    }
    if (selectedView === "week") {
      return { start: startOfWeek(selectedAnchor), days: 7 };
    }
    if (selectedView === "multiweek") {
      return { start: startOfWeek(selectedAnchor), days: 21 };
    }
    return { start: startOfWeek(startOfMonth(selectedAnchor)), days: 42 };
  }

  function daysFor(start: Date, count: number, month?: Date): Day[] {
    return Array.from({ length: count }, (_, index) => {
      const date = addDays(start, index);
      return {
        date,
        key: dayKey(date),
        inMonth: month ? date.getMonth() === month.getMonth() : true
      };
    });
  }

  function newDraft(date: Date): EventDraft {
    return {
      summary: "",
      date: dayKey(date),
      start: "09:00",
      end: "10:00",
      allDay: false,
      location: "",
      description: "",
      recurrence: ""
    };
  }

  function isTauri(): boolean {
    return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  }

  function hour12Option(): boolean | undefined {
    return appearance.timeFormat === "system"
      ? undefined
      : appearance.timeFormat === "12h";
  }

  function formatTime(date: Date): string {
    return new Intl.DateTimeFormat(undefined, {
      hour: "numeric",
      minute: "2-digit",
      hour12: hour12Option()
    }).format(date);
  }

  function formatHour(hour: number): string {
    return new Intl.DateTimeFormat(undefined, {
      hour: "numeric",
      minute: "2-digit",
      hour12: hour12Option()
    }).format(new Date(2000, 0, 1, hour));
  }

  function formatDate(date: Date): string {
    return new Intl.DateTimeFormat(undefined, {
      weekday: "short",
      month: "short",
      day: "numeric"
    }).format(date);
  }

  function formatMonth(date: Date): string {
    return new Intl.DateTimeFormat(undefined, {
      month: "long",
      year: "numeric"
    }).format(date);
  }

  function formatRangeTitle(): string {
    const { start, days } = rangeFor(view, anchor);
    if (view === "day") {
      return new Intl.DateTimeFormat(undefined, {
        weekday: "long",
        month: "long",
        day: "numeric",
        year: "numeric"
      }).format(start);
    }
    if (view === "month") {
      return formatMonth(startOfMonth(anchor));
    }
    const end = addDays(start, days - 1);
    const sameYear = start.getFullYear() === end.getFullYear();
    const startLabel = new Intl.DateTimeFormat(undefined, {
      month: "short",
      day: "numeric",
      year: sameYear ? undefined : "numeric"
    }).format(start);
    const endLabel = new Intl.DateTimeFormat(undefined, {
      month: "short",
      day: "numeric",
      year: "numeric"
    }).format(end);
    return `${startLabel} – ${endLabel}`;
  }

  function eventDate(event: CalendarEvent): Date {
    return new Date(event.startsAt * 1000);
  }

  function eventDayKey(event: CalendarEvent): string {
    return event.allDay ? utcDayKey(eventDate(event)) : dayKey(eventDate(event));
  }

  function eventTimeLabel(event: CalendarEvent): string {
    if (event.allDay) return "All day";
    const start = formatTime(eventDate(event));
    if (!event.endsAt) return start;
    return `${start} – ${formatTime(new Date(event.endsAt * 1000))}`;
  }

  function eventColor(event: CalendarEvent): string {
    return event.calendarColor ?? "var(--accent)";
  }

  function timeValue(date: Date): string {
    return `${String(date.getHours()).padStart(2, "0")}:${String(
      date.getMinutes()
    ).padStart(2, "0")}`;
  }

  function minutesValue(totalMinutes: number): string {
    return `${String(Math.floor(totalMinutes / 60)).padStart(2, "0")}:${String(
      totalMinutes % 60
    ).padStart(2, "0")}`;
  }

  function formatInputDate(key: string): string {
    const [year, month, day] = key.split("-");
    const values: Record<string, string> = { year, month, day };
    return DATE_INPUT_ORDER.map((part) => values[part]).join("/");
  }

  function parseInputDate(value: string): string | null {
    const trimmed = value.trim();
    if (!trimmed) return null;
    const iso = /^(\d{4})-(\d{1,2})-(\d{1,2})$/.exec(trimmed);
    let year: number;
    let month: number;
    let day: number;
    if (iso) {
      year = Number(iso[1]);
      month = Number(iso[2]);
      day = Number(iso[3]);
    } else {
      const numeric = /^(\d{1,2})[\/.\-](\d{1,2})[\/.\-](\d{2,4})$/.exec(
        trimmed
      );
      if (!numeric) return null;
      const first = Number(numeric[1]);
      const second = Number(numeric[2]);
      year = Number(numeric[3]);
      if (year < 100) year += 2000;
      const monthFirst = DATE_INPUT_ORDER[0] === "month";
      month = monthFirst ? first : second;
      day = monthFirst ? second : first;
      if (month > 12 && day <= 12) {
        [month, day] = [day, month];
      }
    }
    if (month < 1 || month > 12 || day < 1 || day > 31) return null;
    const date = new Date(year, month - 1, day);
    if (
      date.getFullYear() !== year ||
      date.getMonth() !== month - 1 ||
      date.getDate() !== day
    ) {
      return null;
    }
    return `${String(year).padStart(4, "0")}-${String(month).padStart(
      2,
      "0"
    )}-${String(day).padStart(2, "0")}`;
  }

  function formatTimeText(value: string): string {
    const [hour, minute] = value.split(":").map(Number);
    return formatTime(new Date(2000, 0, 1, hour || 0, minute || 0));
  }

  function parseTimeText(value: string): string | null {
    const match = /^(\d{1,2})(?::(\d{1,2}))?\s*([ap]\.?m\.?)?$/i.exec(
      value.trim()
    );
    if (!match) return null;
    let hour = Number(match[1]);
    const minute = Number(match[2] ?? "0");
    const meridiem = match[3]?.toLowerCase().replace(/\./g, "");
    if (minute < 0 || minute > 59) return null;
    if (meridiem) {
      if (hour < 1 || hour > 12) return null;
      hour %= 12;
      if (meridiem === "pm") hour += 12;
    } else if (hour > 23) {
      return null;
    }
    return minutesValue(hour * 60 + minute);
  }

  function syncEditorInputs(): void {
    dateInput = formatInputDate(draftEvent.date);
    startInput = formatTimeText(draftEvent.start);
    endInput = formatTimeText(draftEvent.end);
  }

  function handleDateInput(value: string): void {
    dateInput = value;
    const parsed = parseInputDate(value);
    if (parsed) {
      draftEvent.date = parsed;
      datePickerCursor = startOfMonth(parseDayKey(parsed));
    }
  }

  function handleDateBlur(): void {
    const parsed = parseInputDate(dateInput);
    if (parsed) {
      draftEvent.date = parsed;
      dateInput = formatInputDate(parsed);
    }
  }

  function handleTimeInput(kind: "start" | "end", value: string): void {
    if (kind === "start") {
      startInput = value;
      const parsed = parseTimeText(value);
      if (parsed) draftEvent.start = parsed;
    } else {
      endInput = value;
      const parsed = parseTimeText(value);
      if (parsed) draftEvent.end = parsed;
    }
  }

  function handleTimeBlur(kind: "start" | "end"): void {
    const value = kind === "start" ? startInput : endInput;
    const parsed = parseTimeText(value);
    if (!parsed) return;
    if (kind === "start") {
      draftEvent.start = parsed;
      startInput = formatTimeText(parsed);
    } else {
      draftEvent.end = parsed;
      endInput = formatTimeText(parsed);
    }
  }

  function toggleDatePicker(event: MouseEvent): void {
    if (showDatePicker) {
      showDatePicker = false;
      return;
    }
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const width = 276;
    const height = 302;
    const gap = 10;
    let left = Math.max(
      gap,
      Math.min(rect.left, window.innerWidth - width - gap)
    );
    let top = rect.bottom + 6;
    if (top + height > window.innerHeight - gap) {
      top = Math.max(gap, rect.top - height - 6);
    }
    datePickerCursor = startOfMonth(parseDayKey(draftEvent.date));
    datePickerPosition = { left, top };
    showDatePicker = true;
  }

  function selectEditorDate(date: Date): void {
    const key = dayKey(date);
    draftEvent.date = key;
    dateInput = formatInputDate(key);
    datePickerCursor = startOfMonth(date);
    showDatePicker = false;
  }

  function shiftDatePickerMonth(direction: number): void {
    datePickerCursor = new Date(
      datePickerCursor.getFullYear(),
      datePickerCursor.getMonth() + direction,
      1
    );
  }

  function recurrenceLabel(rule: string): string {
    if (!rule) return "";
    const frequency = /FREQ=([^;]+)/i.exec(rule)?.[1]?.toLowerCase() ?? "";
    const interval = Number(/INTERVAL=(\d+)/i.exec(rule)?.[1] ?? "1");
    const unit =
      frequency === "daily"
        ? "day"
        : frequency === "weekly"
          ? "week"
          : frequency === "monthly"
            ? "month"
            : frequency === "yearly"
              ? "year"
              : frequency || "period";
    return interval > 1 ? `Every ${interval} ${unit}s` : `Every ${unit}`;
  }

  function eventMinutes(event: CalendarEvent): { start: number; end: number } {
    const start = eventDate(event);
    const end = event.endsAt ? new Date(event.endsAt * 1000) : null;
    const startMinutes = start.getHours() * 60 + start.getMinutes();
    const endMinutes = end
      ? end.getDate() === start.getDate()
        ? end.getHours() * 60 + end.getMinutes()
        : 24 * 60
      : startMinutes + 60;
    return {
      start: Math.max(0, startMinutes),
      end: Math.max(startMinutes + 20, Math.min(24 * 60, endMinutes))
    };
  }

  function addOccurrenceDays(date: Date, days: number, allDay: boolean): Date {
    const next = new Date(date);
    if (allDay) next.setUTCDate(next.getUTCDate() + days);
    else next.setDate(next.getDate() + days);
    return next;
  }

  function addOccurrenceMonths(
    date: Date,
    months: number,
    allDay: boolean
  ): Date {
    const next = new Date(date);
    if (allDay) {
      const day = next.getUTCDate();
      next.setUTCDate(1);
      next.setUTCMonth(next.getUTCMonth() + months);
      const lastDay = new Date(
        Date.UTC(next.getUTCFullYear(), next.getUTCMonth() + 1, 0)
      ).getUTCDate();
      next.setUTCDate(Math.min(day, lastDay));
    } else {
      const day = next.getDate();
      next.setDate(1);
      next.setMonth(next.getMonth() + months);
      const lastDay = new Date(
        next.getFullYear(),
        next.getMonth() + 1,
        0
      ).getDate();
      next.setDate(Math.min(day, lastDay));
    }
    return next;
  }

  function expandEvent(
    event: CalendarEvent,
    windowStart: Date,
    windowEnd: Date
  ): CalendarEvent[] {
    if (!event.recurrence?.trim()) return [event];
    const frequency = /FREQ=([^;]+)/i.exec(event.recurrence)?.[1]?.toUpperCase();
    if (!frequency) return [event];
    const interval = Math.max(
      1,
      Number(/INTERVAL=(\d+)/i.exec(event.recurrence)?.[1] ?? "1")
    );
    const master = eventDate(event);
    const duration = Math.max(
      60_000,
      event.endsAt !== null
        ? (event.endsAt - event.startsAt) * 1000
        : event.allDay
          ? 86_400_000
          : 3_600_000
    );
    const result: CalendarEvent[] = [];
    const push = (start: Date): void => {
      result.push({
        ...event,
        startsAt: Math.floor(start.getTime() / 1000),
        endsAt: event.endsAt === null
          ? null
          : Math.floor((start.getTime() + duration) / 1000),
        occurrenceKey: `${event.id}:${start.getTime()}`,
        seriesStartsAt: event.startsAt,
        seriesEndsAt: event.endsAt
      });
    };
    const inWindow = (start: Date): boolean =>
      start.getTime() < windowEnd.getTime() &&
      start.getTime() + duration >= windowStart.getTime();

    if (frequency === "DAILY") {
      let cursor = new Date(master);
      for (let index = 0; index < 10_000 && cursor < windowEnd; index += 1) {
        if (inWindow(cursor)) push(new Date(cursor));
        cursor = addOccurrenceDays(cursor, interval, event.allDay);
      }
      return result;
    }

    if (frequency === "WEEKLY") {
      const byDay = /BYDAY=([^;]+)/i.exec(event.recurrence)?.[1];
      if (!byDay) {
        let cursor = new Date(master);
        for (let index = 0; index < 2_000 && cursor < windowEnd; index += 1) {
          if (inWindow(cursor)) push(new Date(cursor));
          cursor = addOccurrenceDays(cursor, interval * 7, event.allDay);
        }
        return result;
      }
      const dayMap: Record<string, number> = {
        SU: 0,
        MO: 1,
        TU: 2,
        WE: 3,
        TH: 4,
        FR: 5,
        SA: 6
      };
      const days = byDay
        .split(",")
        .map((day) => dayMap[day.trim().toUpperCase()])
        .filter((day): day is number => day !== undefined);
      if (days.length === 0) return [event];
      const weekStart = new Date(master);
      if (event.allDay) {
        weekStart.setUTCDate(weekStart.getUTCDate() - weekStart.getUTCDay());
        weekStart.setUTCHours(0, 0, 0, 0);
      } else {
        weekStart.setDate(weekStart.getDate() - weekStart.getDay());
        weekStart.setHours(master.getHours(), master.getMinutes(), 0, 0);
      }
      for (let week = 0; week < 1_000; week += 1) {
        let reachedWindowEnd = false;
        for (const day of days) {
          const start = addOccurrenceDays(
            weekStart,
            week * interval * 7 + day,
            event.allDay
          );
          if (start.getTime() >= windowEnd.getTime()) {
            reachedWindowEnd = true;
            break;
          }
          if (start.getTime() >= master.getTime() && inWindow(start)) {
            push(start);
          }
        }
        if (reachedWindowEnd) break;
      }
      return result;
    }

    const monthStep =
      frequency === "MONTHLY"
        ? interval
        : frequency === "YEARLY"
          ? interval * 12
          : 0;
    if (monthStep > 0) {
      let cursor = new Date(master);
      for (let index = 0; index < 2_000 && cursor < windowEnd; index += 1) {
        if (inWindow(cursor)) push(new Date(cursor));
        cursor = addOccurrenceMonths(cursor, monthStep, event.allDay);
      }
      return result;
    }

    return [event];
  }

  let range = $derived(rangeFor(view, anchor));
  let viewDays = $derived(
    daysFor(
      range.start,
      range.days,
      view === "month" ? startOfMonth(anchor) : undefined
    )
  );
  let monthWeeks = $derived.by<Day[][]>(() => {
    if (view !== "month") return [];
    return Array.from({ length: 6 }, (_, index) =>
      viewDays.slice(index * 7, index * 7 + 7)
    );
  });
  let miniWeeks = $derived.by<Day[][]>(() => {
    const start = startOfWeek(startOfMonth(miniCursor));
    const days = daysFor(start, 42, miniCursor);
    return Array.from({ length: 6 }, (_, index) =>
      days.slice(index * 7, index * 7 + 7)
    );
  });
  let visibleEvents = $derived.by(() => {
    const windowStart = range.start;
    const windowEnd = addDays(range.start, range.days);
    return events
      .filter((event) => !hiddenCalendars.has(event.calendarId))
      .flatMap((event) => expandEvent(event, windowStart, windowEnd));
  });
  let connectedAccounts = $derived(
    Array.from(new Set(calendars.map((calendar) => calendar.calendarAccountId)))
  );
  let writableCalendars = $derived(calendars.filter((calendar) => !calendar.readOnly));
  let selectedCalendar = $derived(
    calendars.find((calendar) => calendar.id === selectedCalendarId) ?? null
  );

  function eventsOn(key: string): CalendarEvent[] {
    return visibleEvents
      .filter((event) => eventDayKey(event) === key)
      .sort((left, right) => left.startsAt - right.startsAt);
  }

  function timedEventsOn(key: string): CalendarEvent[] {
    return eventsOn(key).filter((event) => !event.allDay);
  }

  function allDayOn(key: string): CalendarEvent[] {
    return eventsOn(key).filter((event) => event.allDay);
  }

  function columnTemplate(count: number): string {
    return `54px repeat(${count}, minmax(112px, 1fr))`;
  }

  async function loadCalendars(): Promise<void> {
    if (!isTauri() || app.activeSpaceId === null) {
      calendars = [];
      loading = false;
      return;
    }
    calendars = await api.listCalendars(app.activeSpaceId);
    if (
      selectedCalendarId === null ||
      !calendars.some((calendar) => calendar.id === selectedCalendarId)
    ) {
      selectedCalendarId =
        calendars.find((calendar) => !calendar.readOnly)?.id ??
        calendars[0]?.id ??
        null;
    }
  }

  async function loadRange(): Promise<void> {
    if (!isTauri() || app.activeSpaceId === null) {
      events = [];
      return;
    }
    const request = ++rangeRequest;
    const currentRange = rangeFor(view, anchor);
    const from = Math.floor(currentRange.start.getTime() / 1000);
    const to = Math.floor(addDays(currentRange.start, currentRange.days).getTime() / 1000);
    const next = await api.listCalendarEvents(from, to, app.activeSpaceId, 1000);
    if (request === rangeRequest) events = next;
  }

  async function load(): Promise<void> {
    const spaceId = app.activeSpaceId;
    loading = true;
    error = null;
    try {
      await loadCalendars();
      if (calendars.length > 0) {
        await sync(
          Array.from(new Set(calendars.map((calendar) => calendar.calendarAccountId)))
        );
      } else {
        await loadRange();
      }
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      loadedSpaceId = spaceId;
      loading = false;
    }
  }

  async function sync(accountIds = connectedAccounts): Promise<void> {
    if (syncing || accountIds.length === 0) return;
    syncing = true;
    error = null;
    try {
      const failures: string[] = [];
      for (const accountId of accountIds) {
        try {
          const result = await api.syncCalendar(accountId);
          failures.push(...result.errors);
        } catch (cause) {
          failures.push(cause instanceof Error ? cause.message : String(cause));
        }
      }
      await loadCalendars();
      await loadRange();
      if (failures.length > 0) error = failures[0];
    } finally {
      syncing = false;
    }
  }

  async function accountAdded(account: CalendarAccount): Promise<void> {
    showAddAccount = false;
    error = null;
    try {
      await loadCalendars();
      await sync([account.id]);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  function setView(next: CalendarView): void {
    view = next;
    miniCursor = new Date(anchor.getFullYear(), anchor.getMonth(), 1);
    if (next !== "month") {
      void tick().then(scrollToMorning);
    }
    void loadRange().catch((cause) => {
      error = cause instanceof Error ? cause.message : String(cause);
    });
  }

  function shiftView(direction: number): void {
    const next = new Date(anchor);
    if (view === "day") next.setDate(next.getDate() + direction);
    else if (view === "week") next.setDate(next.getDate() + direction * 7);
    else if (view === "multiweek") next.setDate(next.getDate() + direction * 21);
    else next.setMonth(next.getMonth() + direction);
    anchor = startOfDay(next);
    miniCursor = new Date(anchor.getFullYear(), anchor.getMonth(), 1);
    void loadRange().catch((cause) => {
      error = cause instanceof Error ? cause.message : String(cause);
    });
  }

  function goToday(): void {
    anchor = startOfDay(new Date());
    miniCursor = new Date(anchor.getFullYear(), anchor.getMonth(), 1);
    void loadRange().catch((cause) => {
      error = cause instanceof Error ? cause.message : String(cause);
    });
  }

  function shiftMiniMonth(direction: number): void {
    miniCursor = new Date(
      miniCursor.getFullYear(),
      miniCursor.getMonth() + direction,
      1
    );
  }

  function selectMiniDay(date: Date): void {
    anchor = startOfDay(date);
    miniCursor = new Date(anchor.getFullYear(), anchor.getMonth(), 1);
    void loadRange().catch((cause) => {
      error = cause instanceof Error ? cause.message : String(cause);
    });
  }

  function toggleCalendar(calendarId: number): void {
    const next = new Set(hiddenCalendars);
    if (next.has(calendarId)) next.delete(calendarId);
    else next.add(calendarId);
    hiddenCalendars = next;
  }

  function openCalendarMenu(event: MouseEvent, calendar: CalendarSummary): void {
    event.preventDefault();
    event.stopPropagation();
    calendarMenu = {
      x: Math.min(event.clientX, window.innerWidth - 190),
      y: Math.min(event.clientY, window.innerHeight - 54),
      calendar
    };
  }

  function openGridNew(
    context: MouseEvent,
    date: Date,
    hour = 9,
    minute = 0
  ): void {
    context.preventDefault();
    context.stopPropagation();
    selectedSlot = { dateKey: dayKey(date), minutes: hour * 60 + minute };
    gridMenu = {
      x: Math.min(context.clientX, window.innerWidth - 190),
      y: Math.min(context.clientY, window.innerHeight - 104),
      date,
      hour,
      minute
    };
  }

  function openGridEvent(context: MouseEvent, event: CalendarEvent): void {
    context.preventDefault();
    context.stopPropagation();
    selectedSlot = null;
    gridMenu = {
      x: Math.min(context.clientX, window.innerWidth - 190),
      y: Math.min(context.clientY, window.innerHeight - 104),
      event
    };
  }

  function placeEventDetails(
    event: CalendarEvent,
    anchorLeft: number,
    anchorRight: number,
    anchorTop: number
  ): void {
    const width = 360;
    const height = Math.min(420, window.innerHeight * 0.7);
    const gap = 12;
    const viewportWidth = window.innerWidth;
    const viewportHeight = window.innerHeight;
    let nextLeft = anchorRight + gap;
    if (nextLeft + width > viewportWidth - gap) {
      nextLeft = anchorLeft - width - gap;
    }
    nextLeft = Math.max(
      gap,
      Math.min(nextLeft, viewportWidth - width - gap)
    );
    const nextTop = Math.max(
      gap,
      Math.min(anchorTop, viewportHeight - height - gap)
    );
    selectedEvent = event;
    eventPopover = { left: nextLeft, top: nextTop };
  }

  function openEventFromElement(
    event: CalendarEvent,
    element: HTMLElement
  ): void {
    const rect = element.getBoundingClientRect();
    placeEventDetails(event, rect.left, rect.right, rect.top);
  }

  function openEventFromPoint(
    event: CalendarEvent,
    x: number,
    y: number
  ): void {
    placeEventDetails(event, x, x, y);
  }

  function closeEventDetails(): void {
    selectedEvent = null;
    eventPopover = null;
  }

  async function unsubscribeCalendar(calendar: CalendarSummary): Promise<void> {
    calendarMenu = null;
    try {
      await api.unsubscribeCalendar(calendar.id);
      await loadCalendars();
      await loadRange();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  function openCreate(date = anchor, hour?: number, minute = 0): void {
    createError = null;
    editingEventId = null;
    selectedSlot = null;
    if (writableCalendars.length === 0) {
      error = "All calendars are read-only.";
      return;
    }
    draftEvent = newDraft(date);
    if (hour !== undefined) {
      const startMinutes = hour * 60 + minute;
      const endMinutes = Math.min(23 * 60 + 59, startMinutes + 60);
      draftEvent.start = minutesValue(startMinutes);
      draftEvent.end = minutesValue(endMinutes);
    }
    if (!selectedCalendar || selectedCalendar.readOnly) {
      selectedCalendarId = writableCalendars[0]?.id ?? null;
    }
    syncEditorInputs();
    showDatePicker = false;
    showCreate = true;
  }

  function openEdit(event: CalendarEvent): void {
    if (event.readOnly) {
      openEventFromPoint(event, window.innerWidth / 2, 100);
      return;
    }
    closeEventDetails();
    const seriesStart = event.seriesStartsAt ?? event.startsAt;
    const seriesEnd =
      event.seriesEndsAt !== undefined ? event.seriesEndsAt : event.endsAt;
    const starts = new Date(seriesStart * 1000);
    const ends = seriesEnd !== null && seriesEnd !== undefined
      ? new Date(seriesEnd * 1000)
      : null;
    draftEvent = {
      summary: event.summary,
      date: event.allDay ? utcDayKey(starts) : dayKey(starts),
      start: timeValue(starts),
      end: timeValue(ends ?? new Date(starts.getTime() + 3_600_000)),
      allDay: event.allDay,
      location: event.location,
      description: event.description,
      recurrence: event.recurrence ?? ""
    };
    selectedCalendarId = event.calendarId;
    editingEventId = event.id;
    selectedEvent = null;
    createError = null;
    syncEditorInputs();
    showDatePicker = false;
    showCreate = true;
  }

  function closeEditor(): void {
    showCreate = false;
    showDatePicker = false;
    editingEventId = null;
    createError = null;
  }

  async function createEvent(): Promise<void> {
    if (creating || selectedCalendarId === null) return;
    if (selectedCalendar?.readOnly) {
      createError = "This calendar is read-only.";
      return;
    }
    const summary = draftEvent.summary.trim();
    if (!summary) {
      createError = "Give the event a title.";
      return;
    }
    const parsedDate = parseInputDate(dateInput);
    if (!parsedDate) {
      createError = "Enter a valid date.";
      return;
    }
    draftEvent.date = parsedDate;
    const [year, month, day] = parsedDate.split("-").map(Number);
    let startsAt: number;
    let endsAt: number;
    if (draftEvent.allDay) {
      startsAt = Math.floor(Date.UTC(year, month - 1, day) / 1000);
      endsAt = Math.floor(Date.UTC(year, month - 1, day + 1) / 1000);
    } else {
      const startValue = parseTimeText(startInput);
      const endValue = parseTimeText(endInput);
      if (!startValue || !endValue) {
        createError = "Enter valid start and end times.";
        return;
      }
      const [startHour, startMinute] = startValue.split(":").map(Number);
      const [endHour, endMinute] = endValue.split(":").map(Number);
      const start = new Date(year, month - 1, day, startHour, startMinute);
      const end = new Date(year, month - 1, day, endHour, endMinute);
      startsAt = Math.floor(start.getTime() / 1000);
      endsAt = Math.floor(end.getTime() / 1000);
      if (endsAt <= startsAt) {
        createError = "The end time must be after the start time.";
        return;
      }
    }
    creating = true;
    createError = null;
    try {
      const input = {
        summary,
        description: draftEvent.description.trim(),
        location: draftEvent.location.trim(),
        startsAt,
        endsAt,
        allDay: draftEvent.allDay,
        recurrence: draftEvent.recurrence
      };
      const result =
        editingEventId === null
          ? await api.createCalendarEvent(selectedCalendarId, input)
          : await api.updateCalendarEvent(editingEventId, input);
      if (result.errors.length > 0) {
        createError = result.errors[0];
      } else {
        closeEditor();
        await loadCalendars();
        await loadRange();
      }
    } catch (cause) {
      createError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      creating = false;
    }
  }

  async function deleteEvent(event: CalendarEvent): Promise<void> {
    if (deletingId !== null) return;
    deletingId = event.id;
    error = null;
    try {
      const result = await api.deleteCalendarEvent(event.id);
      if (result.errors.length > 0) error = result.errors[0];
      else closeEventDetails();
      await loadRange();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      deletingId = null;
    }
  }

  function scrollToMorning(): void {
    if (timeScroll) timeScroll.scrollTop = HOUR_HEIGHT * 8;
  }

  onMount(() => {
    const onAccountsChanged = () => {
      void load();
    };
    window.addEventListener(
      "auramail:calendar-accounts-changed",
      onAccountsChanged
    );
    return () => {
      window.removeEventListener(
        "auramail:calendar-accounts-changed",
        onAccountsChanged
      );
    };
  });

  $effect(() => {
    const spaceId = app.activeSpaceId;
    if (spaceId === null || spaceId === loadedSpaceId) return;
    void load().then(async () => {
      await tick();
      scrollToMorning();
    });
  });
</script>

<svelte:window
  onpointerdown={(event) => {
    const target = event.target as Element | null;
    if (!target?.closest(".calendar-context-menu")) {
      calendarMenu = null;
      gridMenu = null;
      selectedSlot = null;
    }
    if (selectedEvent && !target?.closest(".event-popover")) {
      closeEventDetails();
    }
    if (
      showDatePicker &&
      !target?.closest(".date-picker") &&
      !target?.closest(".date-field")
    ) {
      showDatePicker = false;
    }
  }}
  onkeydown={(event) => {
    if (event.key !== "Escape") return;
    if (showDatePicker) {
      showDatePicker = false;
    } else if (calendarMenu || gridMenu) {
      calendarMenu = null;
      gridMenu = null;
      selectedSlot = null;
    } else if (showCreate) {
      closeEditor();
    } else if (selectedEvent) {
      closeEventDetails();
    }
  }}
/>

<section class="calendar-workspace" aria-label="Calendar">
  <aside class="calendar-sidebar">
    <button
      class="new-event-button"
      type="button"
      disabled={calendars.length === 0}
      onclick={() => openCreate()}
    >
      <Plus size={15} />
      New Event
    </button>

    <section class="mini-calendar" aria-label="Mini calendar">
      <div class="mini-head">
        <button
          class="mini-nav"
          type="button"
          aria-label="Previous month"
          onclick={() => shiftMiniMonth(-1)}
        >
          <ChevronLeft size={14} />
        </button>
        <strong>{formatMonth(miniCursor)}</strong>
        <button
          class="mini-nav"
          type="button"
          aria-label="Next month"
          onclick={() => shiftMiniMonth(1)}
        >
          <ChevronRight size={14} />
        </button>
      </div>
      <div class="mini-weekdays">
        <span class="week-number-head">CW</span>
        {#each WEEKDAYS_SHORT as weekday}
          <span>{weekday}</span>
        {/each}
      </div>
      <div class="mini-weeks">
        {#each miniWeeks as week}
          <div class="mini-week">
            <span class="week-number">{weekNumber(week[0].date)}</span>
            {#each week as day}
              <button
                class="mini-day"
                class:outside={!day.inMonth}
                class:selected={day.key === dayKey(anchor)}
                class:today={day.key === dayKey(new Date())}
                type="button"
                aria-label={formatDate(day.date)}
                onclick={() => selectMiniDay(day.date)}
              >
                {day.date.getDate()}
              </button>
            {/each}
          </div>
        {/each}
      </div>
    </section>

    <section class="calendar-list-panel">
      <div class="section-heading">
        <span>Calendars</span>
        <button
          class="mini-nav"
          type="button"
          aria-label="Refresh calendars"
          title="Refresh calendars"
          disabled={syncing}
          onclick={() => void sync()}
        >
          {#if syncing}
            <LoaderCircle class="spin" size={13} />
          {:else}
            <RefreshCw size={13} />
          {/if}
        </button>
      </div>
      {#if calendars.length === 0}
        <p class="sidebar-hint">No calendars connected.</p>
      {:else}
        <div class="calendar-list">
          {#each calendars as calendar (calendar.id)}
            <button
              class="calendar-row"
              class:hidden={hiddenCalendars.has(calendar.id)}
              type="button"
              aria-pressed={!hiddenCalendars.has(calendar.id)}
              onclick={() => toggleCalendar(calendar.id)}
              oncontextmenu={(event) => openCalendarMenu(event, calendar)}
            >
              <i style:background={calendar.color ?? "var(--accent)"}></i>
              <span>{calendar.name}</span>
              {#if calendar.readOnly}
                <Lock size={12} aria-label="Read-only calendar" />
              {/if}
              {#if hiddenCalendars.has(calendar.id)}
                <EyeOff size={13} />
              {:else}
                <Eye size={13} />
              {/if}
            </button>
          {/each}
        </div>
      {/if}
      <button
        class="add-calendar-button"
        type="button"
        onclick={() => (showAddAccount = true)}
      >
        <Plus size={14} />
        Add Calendar…
      </button>
    </section>
  </aside>

  <main class="calendar-main">
    <header class="calendar-toolbar">
      <div class="toolbar-left">
        <button
          class="icon-button"
          type="button"
          aria-label="Previous"
          onclick={() => shiftView(-1)}
        >
          <ChevronLeft size={16} />
        </button>
        <button
          class="icon-button"
          type="button"
          aria-label="Next"
          onclick={() => shiftView(1)}
        >
          <ChevronRight size={16} />
        </button>
        <button class="today-button" type="button" onclick={goToday}>Today</button>
        <h1>{formatRangeTitle()}</h1>
        {#if view === "week" || view === "multiweek"}
          <span class="week-label">CW {weekNumber(range.start)}</span>
        {/if}
      </div>
      <div class="toolbar-right">
        <div class="view-switcher" role="group" aria-label="Calendar view">
          {#each ["day", "week", "multiweek", "month"] as option (option)}
            <button
              type="button"
              class:active={view === option}
              aria-pressed={view === option}
              onclick={() => setView(option as CalendarView)}
            >
              {option === "multiweek" ? "Multiweek" : option[0].toUpperCase() + option.slice(1)}
            </button>
          {/each}
        </div>
        <button
          class="icon-button"
          type="button"
          aria-label="Refresh calendar"
          title="Refresh calendar"
          disabled={syncing}
          onclick={() => void sync()}
        >
          {#if syncing}
            <LoaderCircle class="spin" size={16} />
          {:else}
            <RefreshCw size={16} />
          {/if}
        </button>
      </div>
    </header>

    {#if error}
      <div class="calendar-error" role="alert">
        <span>{error}</span>
        <button type="button" aria-label="Dismiss error" onclick={() => (error = null)}>
          <X size={14} />
        </button>
      </div>
    {/if}

    {#if loading}
      <div class="calendar-empty">
        <LoaderCircle class="spin" size={20} />
        Loading calendar…
      </div>
    {:else if calendars.length === 0}
      <div class="calendar-empty">
        <CalendarDays size={30} />
        <strong>No calendars connected</strong>
        <span>Add a CalDAV calendar from the sidebar to start planning.</span>
        <button class="primary-button" type="button" onclick={() => (showAddAccount = true)}>
          <Plus size={15} />
          Add Calendar
        </button>
      </div>
    {:else if view === "month"}
      <div class="month-view">
        <div class="month-header-row">
          <span class="week-number-head">CW</span>
          {#each WEEKDAYS as weekday}
            <span>{weekday}</span>
          {/each}
        </div>
        <div class="month-weeks">
          {#each monthWeeks as week}
            <div class="month-week">
              <span class="week-number">{weekNumber(week[0].date)}</span>
              {#each week as day}
                {@const dayEvents = eventsOn(day.key)}
                <div
                  class="month-day"
                  class:outside={!day.inMonth}
                  class:today={day.key === dayKey(new Date())}
                  role="button"
                  tabindex="0"
                  aria-label={formatDate(day.date)}
                  onclick={() => {
                    anchor = startOfDay(day.date);
                    miniCursor = new Date(anchor.getFullYear(), anchor.getMonth(), 1);
                    void loadRange();
                  }}
                  onkeydown={(keyboard) => {
                    if (keyboard.key === "Enter" || keyboard.key === " ") {
                      keyboard.preventDefault();
                      anchor = startOfDay(day.date);
                      miniCursor = new Date(anchor.getFullYear(), anchor.getMonth(), 1);
                      void loadRange();
                    }
                  }}
                  ondblclick={() => openCreate(day.date)}
                  oncontextmenu={(event) => openGridNew(event, day.date)}
                >
                  <span class="month-day-number">{day.date.getDate()}</span>
                  <span class="month-day-events">
                    {#each dayEvents.slice(0, 3) as event (event.occurrenceKey ?? event.id)}
                      <span
                        class="event-chip"
                        role="button"
                        tabindex="0"
                        style:border-color={eventColor(event)}
                        title={event.summary}
                        onclick={(click) => {
                          click.stopPropagation();
                          openEventFromElement(
                            event,
                            click.currentTarget as HTMLElement
                          );
                        }}
                        ondblclick={(click) => {
                          click.stopPropagation();
                          openEdit(event);
                        }}
                        oncontextmenu={(context) => openGridEvent(context, event)}
                        onkeydown={(keyboard) => {
                          if (keyboard.key === "Enter" || keyboard.key === " ") {
                            keyboard.preventDefault();
                            openEventFromElement(
                              event,
                              keyboard.currentTarget as HTMLElement
                            );
                          }
                        }}
                      >
                        {#if !event.allDay}
                          <time>{formatTime(eventDate(event))}</time>
                        {/if}
                        {event.summary}
                      </span>
                    {/each}
                    {#if dayEvents.length > 3}
                      <span class="more-events">+{dayEvents.length - 3} more</span>
                    {/if}
                  </span>
                </div>
              {/each}
            </div>
          {/each}
        </div>
      </div>
    {:else}
      <div class="time-view">
        <div class="time-scroll" bind:this={timeScroll}>
          <div
            class="time-grid"
            style:grid-template-columns={columnTemplate(viewDays.length)}
          >
            <div class="time-head-spacer"></div>
            {#each viewDays as day}
              <div
                class="day-head"
                class:today={day.key === dayKey(new Date())}
                role="button"
                tabindex="0"
                onclick={() => selectMiniDay(day.date)}
                ondblclick={(event) => {
                  event.stopPropagation();
                  openCreate(day.date);
                }}
                oncontextmenu={(event) => openGridNew(event, day.date)}
                onkeydown={(keyboard) => {
                  if (keyboard.key === "Enter" || keyboard.key === " ") {
                    keyboard.preventDefault();
                    selectMiniDay(day.date);
                  }
                }}
              >
                <span>{WEEKDAYS[day.date.getDay()]}</span>
                <strong>{day.date.getDate()}</strong>
                <span class="all-day-list">
                  {#each allDayOn(day.key).slice(0, 2) as event (event.occurrenceKey ?? event.id)}
                    <span
                      class="all-day-chip"
                      role="button"
                      tabindex="0"
                      style:border-color={eventColor(event)}
                      onclick={(click) => {
                        click.stopPropagation();
                        openEventFromElement(
                          event,
                          click.currentTarget as HTMLElement
                        );
                      }}
                      ondblclick={(click) => {
                        click.stopPropagation();
                        openEdit(event);
                      }}
                      oncontextmenu={(context) => openGridEvent(context, event)}
                      onkeydown={(keyboard) => {
                        if (keyboard.key === "Enter" || keyboard.key === " ") {
                          keyboard.preventDefault();
                          openEventFromElement(
                            event,
                            keyboard.currentTarget as HTMLElement
                          );
                        }
                      }}
                    >
                      {event.summary}
                    </span>
                  {/each}
                </span>
              </div>
            {/each}

            <div class="time-axis" style:height={`${24 * HOUR_HEIGHT}px`}>
              {#each HOURS as hour}
                <span style:top={`${hour * HOUR_HEIGHT}px`}>{formatHour(hour)}</span>
              {/each}
            </div>
            {#each viewDays as day}
              <div
                class="time-column"
                role="group"
                aria-label={`${formatDate(day.date)} time slots`}
                style:height={`${24 * HOUR_HEIGHT}px`}
              >
                <div class="hour-lines" aria-hidden="true"></div>
                <button
                  class="slot-layer"
                  type="button"
                  aria-label={`Create event on ${formatDate(day.date)}`}
                  onclick={(event) => {
                    const rect = event.currentTarget.getBoundingClientRect();
                    const slot =
                      Math.floor(
                        (event.clientY - rect.top) / (HOUR_HEIGHT / 2)
                      ) * 30;
                    const minutes = Math.max(0, Math.min(23 * 60 + 30, slot));
                    selectedSlot = { dateKey: day.key, minutes };
                  }}
                  ondblclick={(event) => {
                    const rect = event.currentTarget.getBoundingClientRect();
                    const slot =
                      Math.floor(
                        (event.clientY - rect.top) / (HOUR_HEIGHT / 2)
                      ) * 30;
                    const minutes = Math.max(0, Math.min(23 * 60 + 30, slot));
                    openCreate(
                      day.date,
                      Math.floor(minutes / 60),
                      minutes % 60
                    );
                  }}
                  oncontextmenu={(event) => {
                    const rect = event.currentTarget.getBoundingClientRect();
                    const slot =
                      Math.floor(
                        (event.clientY - rect.top) / (HOUR_HEIGHT / 2)
                      ) * 30;
                    const minutes = Math.max(0, Math.min(23 * 60 + 30, slot));
                    openGridNew(
                      event,
                      day.date,
                      Math.floor(minutes / 60),
                      minutes % 60
                    );
                  }}
                ></button>
                {#if selectedSlot?.dateKey === day.key}
                  <div
                    class="slot-selection"
                    style:top={`${(selectedSlot.minutes / 60) * HOUR_HEIGHT}px`}
                    style:height={`${HOUR_HEIGHT / 2}px`}
                  ></div>
                {/if}
                {#each timedEventsOn(day.key) as event (event.occurrenceKey ?? event.id)}
                  {@const minutes = eventMinutes(event)}
                  <button
                    class="time-event"
                    style:top={`${(minutes.start / 60) * HOUR_HEIGHT}px`}
                    style:height={`${Math.max(24, ((minutes.end - minutes.start) / 60) * HOUR_HEIGHT)}px`}
                    style:border-left-color={eventColor(event)}
                    type="button"
                    onclick={(click) => {
                      click.stopPropagation();
                      openEventFromElement(
                        event,
                        click.currentTarget as HTMLElement
                      );
                    }}
                    ondblclick={(click) => {
                      click.stopPropagation();
                      openEdit(event);
                    }}
                    oncontextmenu={(context) => openGridEvent(context, event)}
                  >
                    <strong>{event.summary}</strong>
                    <span>{eventTimeLabel(event)}</span>
                  </button>
                {/each}
              </div>
            {/each}
          </div>
        </div>
      </div>
    {/if}
  </main>

  {#if showCreate}
    <div class="dialog-backdrop" role="presentation" onclick={(event) => {
      if (event.target === event.currentTarget) closeEditor();
    }}>
      <form
        class="event-dialog"
        aria-label={editingEventId === null ? "New event" : "Edit event"}
        onsubmit={(event) => {
          event.preventDefault();
          void createEvent();
        }}
      >
        <div class="dialog-head">
          <div>
            <strong>{editingEventId === null ? "New event" : "Edit event"}</strong>
            <span>{formatDate(parseDayKey(draftEvent.date))}</span>
          </div>
          <button
            class="icon-button"
            type="button"
            aria-label="Close event editor"
            onclick={closeEditor}
          >
            <X size={16} />
          </button>
        </div>

        <label class="wide">
          Title
          <input bind:value={draftEvent.summary} placeholder="Event title" required />
        </label>

        <div class="form-row">
          <label>
            Calendar
            <select bind:value={selectedCalendarId} disabled={editingEventId !== null}>
              {#each calendars as calendar (calendar.id)}
                <option value={calendar.id} disabled={calendar.readOnly}>
                  {calendar.name}{calendar.readOnly ? " (read-only)" : ""}
                </option>
              {/each}
            </select>
          </label>
          <label>
            Date
            <span class="date-field">
              <input
                class="date-text"
                value={dateInput}
                autocomplete="off"
                placeholder={formatInputDate(dayKey(new Date()))}
                oninput={(event) => handleDateInput(event.currentTarget.value)}
                onblur={handleDateBlur}
              />
              <button
                class="date-trigger"
                type="button"
                aria-label="Open calendar"
                aria-expanded={showDatePicker}
                onclick={toggleDatePicker}
              >
                <CalendarDays size={15} />
              </button>
            </span>
          </label>
        </div>

        {#if showDatePicker && datePickerPosition}
          <div
            class="date-picker"
            role="dialog"
            aria-label="Choose event date"
            style:left={`${datePickerPosition.left}px`}
            style:top={`${datePickerPosition.top}px`}
          >
            <div class="date-picker-head">
              <button
                type="button"
                aria-label="Previous month"
                onclick={() => shiftDatePickerMonth(-1)}
              >
                <ChevronLeft size={15} />
              </button>
              <strong>{formatMonth(datePickerCursor)}</strong>
              <button
                type="button"
                aria-label="Next month"
                onclick={() => shiftDatePickerMonth(1)}
              >
                <ChevronRight size={15} />
              </button>
            </div>
            <div class="date-picker-weekdays" aria-hidden="true">
              {#each WEEKDAYS_SHORT as weekday}
                <span>{weekday}</span>
              {/each}
            </div>
            <div class="date-picker-grid">
              {#each datePickerDays as day}
                <button
                  type="button"
                  class:outside={!day.inMonth}
                  class:today={day.key === dayKey(new Date())}
                  class:selected={day.key === draftEvent.date}
                  onclick={() => selectEditorDate(day.date)}
                >
                  {day.date.getDate()}
                </button>
              {/each}
            </div>
          </div>
        {/if}

        <label class="check-field">
          <input type="checkbox" bind:checked={draftEvent.allDay} />
          All day
        </label>

        {#if !draftEvent.allDay}
          <div class="form-row">
            <label>
              Starts
              <input
                type="text"
                value={startInput}
                autocomplete="off"
                placeholder={formatTimeText("09:00")}
                oninput={(event) =>
                  handleTimeInput("start", event.currentTarget.value)}
                onblur={() => handleTimeBlur("start")}
              />
            </label>
            <label>
              Ends
              <input
                type="text"
                value={endInput}
                autocomplete="off"
                placeholder={formatTimeText("10:00")}
                oninput={(event) =>
                  handleTimeInput("end", event.currentTarget.value)}
                onblur={() => handleTimeBlur("end")}
              />
            </label>
          </div>
        {/if}

        <fieldset class="repeat-field">
          <legend><Repeat size={13} /> Repeat</legend>
          <select bind:value={draftEvent.recurrence}>
            {#each REPEAT_OPTIONS as option (option.value)}
              <option value={option.value}>{option.label}</option>
            {/each}
            {#if
              draftEvent.recurrence &&
              !REPEAT_OPTIONS.some((option) => option.value === draftEvent.recurrence)
            }
              <option value={draftEvent.recurrence}>
                {recurrenceLabel(draftEvent.recurrence)} (custom)
              </option>
            {/if}
          </select>
          {#if draftEvent.recurrence}
            <span>{recurrenceLabel(draftEvent.recurrence)}</span>
          {/if}
        </fieldset>

        <label>
          Location
          <input bind:value={draftEvent.location} placeholder="Optional" />
        </label>

        <label>
          Description
          <textarea bind:value={draftEvent.description} rows="4" placeholder="Optional"></textarea>
        </label>

        {#if createError}
          <p class="form-error" role="alert">{createError}</p>
        {/if}

        <div class="dialog-actions">
          <button type="button" onclick={closeEditor}>Cancel</button>
          <button
            class="primary-button"
            type="submit"
            disabled={creating || selectedCalendar?.readOnly}
          >
            {selectedCalendar?.readOnly
              ? "Read-only calendar"
              : creating
                ? "Saving…"
                : editingEventId === null
                  ? "Create event"
                  : "Save changes"}
          </button>
        </div>
      </form>
    </div>
  {/if}

  {#if selectedEvent && eventPopover}
    <section
      class="event-detail event-popover"
      aria-label="Event details"
      style:left={`${eventPopover.left}px`}
      style:top={`${eventPopover.top}px`}
    >
      <div class="detail-head">
        <span class="detail-calendar">
          {selectedEvent.calendarName}
          {#if selectedEvent.readOnly}
            <Lock size={12} />
            <span>Read-only</span>
          {/if}
        </span>
        <button
          class="icon-button"
          type="button"
          aria-label="Close event details"
          onclick={closeEventDetails}
        >
          <X size={16} />
        </button>
      </div>

      <h2>{selectedEvent.summary}</h2>

      <div class="detail-meta">
        <Clock size={15} />
        <span>
          {formatDate(eventDate(selectedEvent))} · {eventTimeLabel(selectedEvent)}
        </span>
      </div>

      {#if selectedEvent.recurrence}
        <div class="detail-meta">
          <Repeat size={15} />
          <span>{recurrenceLabel(selectedEvent.recurrence)}</span>
        </div>
      {/if}

      {#if selectedEvent.location}
        <div class="detail-meta">
          <MapPin size={15} />
          <span>{selectedEvent.location}</span>
        </div>
      {/if}

      {#if selectedEvent.description}
        <p class="detail-description">{selectedEvent.description}</p>
      {/if}

      <div class="detail-actions">
        {#if !selectedEvent.readOnly}
          <button
            class="edit-button"
            type="button"
            onclick={() => openEdit(selectedEvent!)}
          >
            <Pencil size={14} />
            Edit
          </button>
        {/if}
        <button
          class="danger-button"
          type="button"
          disabled={deletingId !== null || selectedEvent.readOnly}
          onclick={() => void deleteEvent(selectedEvent!)}
        >
          <Trash2 size={14} />
          {deletingId !== null ? "Deleting…" : "Delete"}
        </button>
      </div>
    </section>
  {/if}

  {#if showAddAccount}
    <CalendarAccountDialog
      spaceId={app.activeSpaceId ?? 1}
      onClose={() => (showAddAccount = false)}
      onAdded={accountAdded}
    />
  {/if}

  {#if calendarMenu}
    <div
      class="calendar-context-menu"
      role="menu"
      style:left={`${calendarMenu.x}px`}
      style:top={`${calendarMenu.y}px`}
    >
      <button
        type="button"
        role="menuitem"
        onclick={() => void unsubscribeCalendar(calendarMenu!.calendar)}
      >
        <Trash2 size={14} />
        Unsubscribe from calendar
      </button>
    </div>
  {/if}

  {#if gridMenu}
    <div
      class="calendar-context-menu grid-context-menu"
      role="menu"
      style:left={`${gridMenu.x}px`}
      style:top={`${gridMenu.y}px`}
    >
      {#if gridMenu.event}
        {#if !gridMenu.event.readOnly}
          <button
            type="button"
            role="menuitem"
            onclick={() => {
              const event = gridMenu!.event!;
              gridMenu = null;
              selectedSlot = null;
              openEdit(event);
            }}
          >
            <Pencil size={14} />
            Edit event
          </button>
          <button
            class="danger-item"
            type="button"
            role="menuitem"
            onclick={() => {
              const event = gridMenu!.event!;
              gridMenu = null;
              selectedSlot = null;
              void deleteEvent(event);
            }}
          >
            <Trash2 size={14} />
            Delete event
          </button>
        {:else}
          <button
            type="button"
            role="menuitem"
            onclick={() => {
              const event = gridMenu!.event!;
              const { x, y } = gridMenu!;
              gridMenu = null;
              selectedSlot = null;
              openEventFromPoint(event, x, y);
            }}
          >
            <CalendarDays size={14} />
            View event
          </button>
        {/if}
      {:else}
        <button
          type="button"
          role="menuitem"
          onclick={() => {
            const menu = gridMenu!;
            gridMenu = null;
            selectedSlot = null;
            if (menu.date) openCreate(menu.date, menu.hour, menu.minute);
          }}
        >
          <Plus size={14} />
          New event
        </button>
      {/if}
    </div>
  {/if}
</section>

<style>
  .calendar-workspace {
    grid-column: 2;
    display: grid;
    grid-template-columns: 272px minmax(0, 1fr);
    min-width: 0;
    min-height: 0;
    height: 100%;
    background: var(--bg);
  }

  .calendar-sidebar {
    display: flex;
    flex-direction: column;
    min-width: 0;
    padding: 12px;
    gap: 14px;
    border-right: 1px solid var(--border);
    background: var(--bg-raised);
    overflow: auto;
  }

  button {
    font: inherit;
    color: var(--text);
    border: 1px solid var(--border);
    background: var(--bg-raised);
    cursor: pointer;
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .new-event-button,
  .primary-button,
  .add-calendar-button,
  .today-button,
  .view-switcher,
  .calendar-row,
  .mini-nav,
  .icon-button {
    display: inline-flex;
    align-items: center;
  }

  .new-event-button {
    justify-content: center;
    gap: 6px;
    width: 100%;
    padding: 8px 10px;
    border-color: var(--accent);
    border-radius: 8px;
    background: var(--accent);
    color: var(--bg);
    font-size: 12px;
    font-weight: 600;
  }

  .new-event-button:hover {
    filter: brightness(1.08);
  }

  .icon-button,
  .mini-nav {
    justify-content: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border-radius: 7px;
    background: transparent;
  }

  .icon-button:hover:not(:disabled),
  .mini-nav:hover:not(:disabled),
  .add-calendar-button:hover {
    background: var(--bg-hover);
  }

  .mini-calendar {
    padding-bottom: 4px;
  }

  .mini-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 4px;
    margin-bottom: 8px;
  }

  .mini-head strong {
    color: var(--text-strong);
    font-size: 12px;
    font-weight: 600;
  }

  .mini-weekdays,
  .mini-week {
    display: grid;
    grid-template-columns: 22px repeat(7, 1fr);
    align-items: center;
  }

  .mini-weekdays {
    margin-bottom: 3px;
    color: var(--muted);
    font-size: 9px;
    text-align: center;
    text-transform: uppercase;
  }

  .week-number-head,
  .week-number {
    color: var(--muted);
    font-size: 9px;
    text-align: center;
  }

  .mini-week {
    min-height: 27px;
  }

  .mini-day {
    display: grid;
    place-items: center;
    width: 100%;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--text);
    font-size: 11px;
  }

  .mini-day:hover {
    background: var(--bg-hover);
  }

  .mini-day.outside {
    color: var(--muted);
    opacity: 0.55;
  }

  .mini-day.today {
    color: var(--accent);
    font-weight: 700;
  }

  .mini-day.selected {
    background: color-mix(in srgb, var(--accent) 18%, var(--bg-raised));
    color: var(--accent);
    font-weight: 700;
  }

  .calendar-list-panel {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .section-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    color: var(--text-strong);
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .section-heading .mini-nav {
    width: 24px;
    height: 24px;
  }

  .sidebar-hint {
    margin: 2px 0 4px;
    color: var(--muted);
    font-size: 11px;
    line-height: 1.4;
  }

  .calendar-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .calendar-row {
    gap: 7px;
    width: 100%;
    padding: 6px 7px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    text-align: left;
    font-size: 12px;
  }

  .calendar-row:hover {
    background: var(--bg-hover);
  }

  .calendar-row.hidden {
    color: var(--muted);
  }

  .calendar-row i {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }

  .calendar-row span {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .calendar-row :global(svg) {
    flex: none;
    color: var(--muted);
  }

  .calendar-context-menu {
    position: fixed;
    z-index: 120;
    min-width: 190px;
    padding: 4px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-raised);
    box-shadow: 0 8px 24px color-mix(in srgb, #000 35%, transparent);
  }

  .calendar-context-menu button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 9px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--text);
    font-size: 11px;
    text-align: left;
  }

  .calendar-context-menu button:hover {
    background: var(--bg-hover);
  }

  .add-calendar-button {
    gap: 5px;
    align-self: flex-start;
    padding: 5px 3px;
    border: 0;
    background: transparent;
    color: var(--muted);
    font-size: 11px;
  }

  .add-calendar-button:hover {
    color: var(--text);
  }

  .event-dialog input,
  .event-dialog select,
  .event-dialog textarea {
    min-width: 0;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--bg-raised);
    color: var(--text);
    padding: 8px 10px;
    font: inherit;
    font-size: 12px;
  }

  .dialog-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
  }

  .dialog-actions button {
    border-radius: 6px;
    padding: 5px 8px;
    background: transparent;
    font-size: 11px;
  }

  .primary-button {
    justify-content: center;
    gap: 5px;
    border-color: var(--accent);
    background: var(--accent);
    color: var(--bg);
    font-weight: 600;
  }

  .dialog-actions .primary-button {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--bg);
  }

  .dialog-actions .primary-button:hover:not(:disabled) {
    filter: brightness(1.06);
  }

  .calendar-main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .calendar-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-height: 48px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-raised);
  }

  .toolbar-left,
  .toolbar-right,
  .day-head,
  .event-chip,
  .all-day-chip,
  .detail-calendar {
    display: flex;
    align-items: center;
  }

  .toolbar-left {
    gap: 4px;
    min-width: 0;
  }

  .toolbar-left h1 {
    margin: 0 0 0 7px;
    color: var(--text-strong);
    font-size: 14px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .today-button {
    padding: 5px 8px;
    border-radius: 6px;
    background: transparent;
    font-size: 11px;
  }

  .week-label {
    color: var(--muted);
    font-size: 10px;
    white-space: nowrap;
  }

  .toolbar-right {
    gap: 8px;
  }

  .view-switcher {
    padding: 2px;
    gap: 2px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--bg);
  }

  .view-switcher button {
    padding: 4px 8px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--muted);
    font-size: 11px;
  }

  .view-switcher button:hover {
    color: var(--text);
  }

  .view-switcher button.active {
    background: var(--bg-hover);
    color: var(--text-strong);
  }

  .calendar-error {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    margin: 10px 14px 0;
    padding: 7px 9px;
    border: 1px solid color-mix(in srgb, var(--danger) 45%, var(--border));
    border-radius: 7px;
    background: color-mix(in srgb, var(--danger) 10%, var(--bg));
    color: var(--danger);
    font-size: 11px;
  }

  .calendar-error button {
    display: grid;
    place-items: center;
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
  }

  .calendar-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 9px;
    flex: 1;
    min-height: 260px;
    padding: 30px;
    color: var(--muted);
    font-size: 12px;
    text-align: center;
  }

  .calendar-empty strong {
    color: var(--text-strong);
    font-size: 14px;
  }

  .calendar-empty span {
    max-width: 360px;
    line-height: 1.5;
  }

  .time-view {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .time-scroll {
    height: 100%;
    overflow: auto;
    scrollbar-gutter: stable;
  }

  .time-grid {
    display: grid;
    grid-template-rows: auto 1fr;
    width: max-content;
    min-width: 100%;
  }

  .time-head-spacer {
    grid-row: 1;
    grid-column: 1;
    position: sticky;
    top: 0;
    z-index: 3;
    height: 64px;
    border-right: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    background: var(--bg-raised);
  }

  .day-head {
    grid-row: 1;
    position: sticky;
    top: 0;
    z-index: 3;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 1px;
    min-width: 0;
    height: 64px;
    padding: 5px 7px;
    border: 0;
    border-right: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    border-radius: 0;
    background: var(--bg-raised);
    text-align: left;
    cursor: pointer;
  }

  .day-head:hover {
    background: var(--bg-hover);
  }

  .day-head > span:first-child {
    color: var(--muted);
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .day-head strong {
    color: var(--text-strong);
    font-size: 12px;
    font-weight: 600;
  }

  .day-head.today strong {
    color: var(--accent);
  }

  .all-day-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    overflow: hidden;
    margin-top: 2px;
  }

  .all-day-chip {
    display: block;
    min-width: 0;
    overflow: hidden;
    padding: 1px 4px;
    border-left: 2px solid var(--accent);
    border-radius: 3px;
    background: color-mix(in srgb, var(--accent) 9%, transparent);
    color: var(--text);
    font-size: 9px;
    line-height: 1.3;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }

  .time-axis {
    grid-row: 2;
    grid-column: 1;
    position: relative;
    border-right: 1px solid var(--border);
    background: var(--bg-raised);
  }

  .time-axis span {
    position: absolute;
    right: 7px;
    color: var(--muted);
    font-size: 9px;
    transform: translateY(-4px);
  }

  .time-column {
    grid-row: 2;
    position: relative;
    min-width: 0;
    border-right: 1px solid var(--border);
    background: color-mix(in srgb, var(--bg) 94%, var(--bg-raised));
  }

  .slot-layer {
    position: absolute;
    inset: 0;
    padding: 0;
    border: 0;
    background: transparent;
    cursor: default;
  }

  .slot-layer:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .hour-lines {
    position: absolute;
    inset: 0;
    background:
      repeating-linear-gradient(
        to bottom,
        transparent 0,
        transparent 27px,
        color-mix(in srgb, var(--border) 55%, transparent) 27px,
        color-mix(in srgb, var(--border) 55%, transparent) 28px
      ),
      repeating-linear-gradient(
        to bottom,
        transparent 0,
        transparent calc(var(--hour-height, 56px) - 1px),
        var(--border) calc(var(--hour-height, 56px) - 1px),
        var(--border) var(--hour-height, 56px)
      );
    pointer-events: none;
  }

  .slot-selection {
    position: absolute;
    left: 0;
    right: 0;
    border-top: 1px solid color-mix(in srgb, var(--accent) 60%, transparent);
    border-bottom: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    pointer-events: none;
  }

  .time-event {
    position: absolute;
    z-index: 1;
    left: 4px;
    right: 4px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    overflow: hidden;
    padding: 4px 5px;
    border: 0;
    border-left: 3px solid var(--accent);
    border-radius: 4px;
    background: color-mix(in srgb, var(--accent) 14%, var(--bg-raised));
    text-align: left;
    box-shadow: 0 1px 2px color-mix(in srgb, var(--border) 30%, transparent);
  }

  .time-event:hover {
    filter: brightness(1.08);
    z-index: 2;
  }

  .time-event strong {
    overflow: hidden;
    color: var(--text-strong);
    font-size: 10px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .time-event span {
    overflow: hidden;
    color: var(--muted);
    font-size: 9px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .month-view {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }

  .month-header-row,
  .month-week {
    display: grid;
    grid-template-columns: 34px repeat(7, minmax(100px, 1fr));
    min-width: 760px;
  }

  .month-header-row {
    position: sticky;
    top: 0;
    z-index: 2;
    border-bottom: 1px solid var(--border);
    background: var(--bg-raised);
  }

  .month-header-row span {
    padding: 8px;
    color: var(--muted);
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    border-right: 1px solid var(--border);
  }

  .month-week {
    min-height: 116px;
  }

  .month-day {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    min-height: 116px;
    padding: 5px;
    border: 0;
    border-right: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    border-radius: 0;
    background: transparent;
    text-align: left;
    overflow: hidden;
    cursor: pointer;
  }

  .month-day:hover {
    background: var(--bg-hover);
  }

  .month-day.outside {
    opacity: 0.45;
  }

  .month-day-number {
    display: grid;
    place-items: center;
    width: 21px;
    height: 21px;
    border-radius: 50%;
    color: var(--text);
    font-size: 10px;
  }

  .month-day.today .month-day-number {
    background: var(--accent);
    color: var(--bg);
    font-weight: 700;
  }

  .month-day-events {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .event-chip {
    display: block;
    min-width: 0;
    overflow: hidden;
    padding: 2px 4px;
    border-left: 2px solid var(--accent);
    border-radius: 3px;
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    color: var(--text);
    font-size: 9px;
    line-height: 1.3;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }

  .event-chip time {
    color: var(--muted);
    margin-right: 3px;
  }

  .more-events {
    color: var(--muted);
    font-size: 9px;
    padding-left: 4px;
  }

  .dialog-backdrop {
    position: fixed;
    z-index: 80;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 24px;
    background: color-mix(in srgb, #000 38%, transparent);
  }

  .event-dialog {
    display: flex;
    flex-direction: column;
    gap: 16px;
    width: min(620px, 100%);
    max-height: min(90vh, 820px);
    overflow: auto;
    padding: 22px;
    border: 1px solid var(--border);
    border-radius: 14px;
    background: var(--bg-raised);
    box-shadow: 0 22px 64px color-mix(in srgb, #000 42%, transparent);
  }

  .event-detail {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: min(360px, calc(100vw - 24px));
    max-height: min(70vh, 420px);
    overflow: auto;
    padding: 18px;
    border: 1px solid var(--border);
    border-radius: 14px;
    background: var(--bg-raised);
    box-shadow: 0 18px 54px color-mix(in srgb, #000 42%, transparent);
  }

  .event-popover {
    position: fixed;
    z-index: 90;
  }

  .dialog-head,
  .detail-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  .dialog-head > div {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .dialog-head strong {
    color: var(--text-strong);
    font-size: 15px;
    font-weight: 650;
  }

  .dialog-head span {
    color: var(--muted);
    font-size: 11px;
  }

  .event-dialog label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    color: var(--muted);
    font-size: 10px;
    font-weight: 550;
  }

  .form-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 12px;
  }

  .event-dialog textarea {
    resize: vertical;
    min-height: 96px;
  }

  .event-dialog input,
  .event-dialog select,
  .event-dialog textarea {
    background: var(--bg);
    color: var(--text);
    border-color: color-mix(in srgb, var(--border) 82%, var(--text));
    color-scheme: inherit;
  }

  .date-field {
    position: relative;
    display: flex;
    align-items: center;
  }

  .date-field .date-text {
    width: 100%;
    padding-right: 36px;
  }

  .date-trigger {
    position: absolute;
    right: 4px;
    display: grid;
    place-items: center;
    width: 27px;
    height: 27px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--muted);
  }

  .date-trigger:hover {
    background: var(--bg-hover);
    color: var(--text);
  }

  .date-picker {
    position: fixed;
    z-index: 140;
    width: 276px;
    padding: 11px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg-raised);
    box-shadow: 0 16px 42px color-mix(in srgb, #000 36%, transparent);
  }

  .date-picker-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 9px;
  }

  .date-picker-head strong {
    color: var(--text-strong);
    font-size: 11px;
    font-weight: 600;
    text-transform: capitalize;
  }

  .date-picker-head button {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--muted);
  }

  .date-picker-head button:hover {
    background: var(--bg-hover);
    color: var(--text);
  }

  .date-picker-weekdays,
  .date-picker-grid {
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
  }

  .date-picker-weekdays {
    margin-bottom: 3px;
    color: var(--muted);
    font-size: 9px;
    text-align: center;
    text-transform: uppercase;
  }

  .date-picker-weekdays span {
    padding: 3px 0;
  }

  .date-picker-grid button {
    display: grid;
    place-items: center;
    width: 100%;
    height: 30px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    font-size: 10px;
  }

  .date-picker-grid button:hover {
    background: var(--bg-hover);
  }

  .date-picker-grid button.outside {
    color: var(--muted);
    opacity: 0.55;
  }

  .date-picker-grid button.today {
    color: var(--accent);
    font-weight: 700;
  }

  .date-picker-grid button.selected {
    background: var(--accent);
    color: var(--bg);
    font-weight: 700;
  }

  :global(html[data-theme-dark="true"]) .event-dialog select {
    color-scheme: dark;
  }

  :global(html[data-theme-dark="false"]) .event-dialog select {
    color-scheme: light;
  }

  .event-dialog select {
    appearance: none;
    padding-right: 26px;
    background-image:
      linear-gradient(45deg, transparent 50%, var(--muted) 50%),
      linear-gradient(135deg, var(--muted) 50%, transparent 50%);
    background-position:
      calc(100% - 13px) calc(50% - 2px),
      calc(100% - 8px) calc(50% - 2px);
    background-size: 5px 5px, 5px 5px;
    background-repeat: no-repeat;
  }

  .event-dialog select option {
    background: var(--bg-raised);
    color: var(--text);
  }

  .event-dialog input:focus,
  .event-dialog select:focus,
  .event-dialog textarea:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 18%, transparent);
  }

  .event-dialog .check-field {
    flex-direction: row;
    align-items: center;
    align-self: flex-start;
    padding: 2px 0;
    color: var(--text);
    font-size: 11px;
  }

  .event-dialog .check-field input {
    width: auto;
    accent-color: var(--accent);
  }

  .repeat-field {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin: 0;
    padding: 12px;
    border: 1px solid color-mix(in srgb, var(--border) 78%, transparent);
    border-radius: 9px;
    background: color-mix(in srgb, var(--bg) 58%, transparent);
  }

  .repeat-field legend {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 0 4px;
    color: var(--text-strong);
    font-size: 10px;
    font-weight: 600;
  }

  .repeat-field select {
    flex: 1;
    min-width: 150px;
  }

  .repeat-field > span {
    color: var(--muted);
    font-size: 10px;
  }

  .form-error {
    margin: 0;
    color: var(--danger);
    font-size: 11px;
  }

  .dialog-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
    padding-top: 14px;
    border-top: 1px solid color-mix(in srgb, var(--border) 72%, transparent);
  }

  .dialog-actions button {
    padding: 7px 12px;
    font-size: 11px;
  }

  .detail-calendar {
    gap: 6px;
    color: var(--muted);
    font-size: 11px;
  }

  .event-detail h2 {
    margin: 0;
    color: var(--text-strong);
    font-size: 20px;
    font-weight: 600;
    line-height: 1.25;
    overflow-wrap: anywhere;
  }

  .detail-meta {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    color: var(--text);
    font-size: 12px;
    line-height: 1.45;
  }

  .detail-meta :global(svg) {
    flex: none;
    margin-top: 1px;
    color: var(--muted);
  }

  .detail-description {
    margin: 0;
    padding: 10px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--bg) 62%, transparent);
    color: var(--text);
    font-size: 12px;
    line-height: 1.55;
    white-space: pre-wrap;
  }

  .detail-actions {
    display: flex;
    gap: 7px;
    justify-content: flex-end;
    padding-top: 10px;
    border-top: 1px solid color-mix(in srgb, var(--border) 72%, transparent);
  }

  .detail-actions button {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 6px 10px;
    border-radius: 7px;
    background: transparent;
    font-size: 11px;
  }

  .detail-actions .edit-button {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--bg);
    font-weight: 600;
  }

  .detail-actions .danger-button {
    color: var(--danger);
    border-color: color-mix(in srgb, var(--danger) 35%, var(--border));
  }

  .calendar-context-menu .danger-item {
    color: var(--danger);
  }

  :global(.spin) {
    animation: spin 900ms linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (max-width: 980px) {
    .calendar-workspace {
      grid-template-columns: 232px minmax(0, 1fr);
    }

    .calendar-toolbar {
      flex-wrap: wrap;
    }

    .week-label {
      display: none;
    }
  }

  @media (max-width: 760px) {
    .calendar-workspace {
      grid-template-columns: minmax(0, 1fr);
    }

    .calendar-sidebar {
      display: none;
    }

    .event-dialog {
      width: 100%;
    }

    .form-row {
      grid-template-columns: 1fr;
    }

    .event-dialog .wide,
    .form-error,
    .dialog-actions {
      grid-column: 1;
    }
  }
</style>
