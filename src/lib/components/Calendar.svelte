<script lang="ts">
  import { onMount, tick } from "svelte";
  import {
    CalendarDays,
    ChevronLeft,
    ChevronRight,
    Eye,
    EyeOff,
    LoaderCircle,
    Plus,
    RefreshCw,
    Trash2,
    X
  } from "lucide-svelte";
  import { api } from "../api";
  import { app } from "../stores/app.svelte";
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
  };

  const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
  const WEEKDAYS_SHORT = ["S", "M", "T", "W", "T", "F", "S"];
  const HOURS = Array.from({ length: 24 }, (_, hour) => hour);
  const HOUR_HEIGHT = 56;

  let calendars = $state<CalendarSummary[]>([]);
  let events = $state<CalendarEvent[]>([]);
  let hiddenCalendars = $state<Set<number>>(new Set());
  let loading = $state(true);
  let syncing = $state(false);
  let creating = $state(false);
  let deletingId = $state<number | null>(null);
  let error = $state<string | null>(null);
  let createError = $state<string | null>(null);
  let showAddAccount = $state(false);
  let showCreate = $state(false);
  let selectedCalendarId = $state<number | null>(null);
  let selectedEvent = $state<CalendarEvent | null>(null);
  let view = $state<CalendarView>("week");
  let anchor = $state(startOfDay(new Date()));
  let miniCursor = $state(
    new Date(new Date().getFullYear(), new Date().getMonth(), 1)
  );
  let timeScroll = $state<HTMLDivElement | null>(null);
  let rangeRequest = 0;
  let draftEvent = $state<EventDraft>(newDraft(startOfDay(new Date())));

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
      description: ""
    };
  }

  function isTauri(): boolean {
    return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  }

  function formatTime(date: Date): string {
    return new Intl.DateTimeFormat(undefined, {
      hour: "numeric",
      minute: "2-digit"
    }).format(date);
  }

  function formatHour(hour: number): string {
    return new Intl.DateTimeFormat(undefined, {
      hour: "numeric",
      minute: "2-digit",
      hour12: undefined
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
  let visibleEvents = $derived(
    events.filter((event) => !hiddenCalendars.has(event.calendarId))
  );
  let connectedAccounts = $derived(
    Array.from(new Set(calendars.map((calendar) => calendar.calendarAccountId)))
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
      selectedCalendarId = calendars[0]?.id ?? null;
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

  function openCreate(date = anchor, hour?: number): void {
    createError = null;
    draftEvent = newDraft(date);
    if (hour !== undefined) {
      const start = String(hour).padStart(2, "0") + ":00";
      const end = String(Math.min(23, hour + 1)).padStart(2, "0") + ":00";
      draftEvent.start = start;
      draftEvent.end = end;
    }
    if (selectedCalendarId === null) {
      selectedCalendarId = calendars[0]?.id ?? null;
    }
    showCreate = true;
  }

  async function createEvent(): Promise<void> {
    if (creating || selectedCalendarId === null) return;
    const summary = draftEvent.summary.trim();
    if (!summary) {
      createError = "Give the event a title.";
      return;
    }
    const [year, month, day] = draftEvent.date.split("-").map(Number);
    let startsAt: number;
    let endsAt: number;
    if (draftEvent.allDay) {
      startsAt = Math.floor(Date.UTC(year, month - 1, day) / 1000);
      endsAt = Math.floor(Date.UTC(year, month - 1, day + 1) / 1000);
    } else {
      const [startHour, startMinute] = draftEvent.start.split(":").map(Number);
      const [endHour, endMinute] = draftEvent.end.split(":").map(Number);
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
      const result = await api.createCalendarEvent(selectedCalendarId, {
        summary,
        description: draftEvent.description.trim(),
        location: draftEvent.location.trim(),
        startsAt,
        endsAt,
        allDay: draftEvent.allDay
      });
      if (result.errors.length > 0) {
        createError = result.errors[0];
      } else {
        showCreate = false;
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
      else selectedEvent = null;
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
    void load().then(async () => {
      await tick();
      scrollToMorning();
    });
  });
</script>

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
            >
              <i style:background={calendar.color ?? "var(--accent)"}></i>
              <span>{calendar.name}</span>
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
                >
                  <span class="month-day-number">{day.date.getDate()}</span>
                  <span class="month-day-events">
                    {#each dayEvents.slice(0, 3) as event (event.id)}
                      <span
                        class="event-chip"
                        role="button"
                        tabindex="0"
                        style:border-color={eventColor(event)}
                        title={event.summary}
                        onclick={(click) => {
                          click.stopPropagation();
                          selectedEvent = event;
                        }}
                        onkeydown={(keyboard) => {
                          if (keyboard.key === "Enter" || keyboard.key === " ") {
                            keyboard.preventDefault();
                            selectedEvent = event;
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
                  {#each allDayOn(day.key).slice(0, 2) as event (event.id)}
                    <span
                      class="all-day-chip"
                      role="button"
                      tabindex="0"
                      style:border-color={eventColor(event)}
                      onclick={(click) => {
                        click.stopPropagation();
                        selectedEvent = event;
                      }}
                      onkeydown={(keyboard) => {
                        if (keyboard.key === "Enter" || keyboard.key === " ") {
                          keyboard.preventDefault();
                          selectedEvent = event;
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
              <div class="time-column" style:height={`${24 * HOUR_HEIGHT}px`}>
                <div class="hour-lines" aria-hidden="true"></div>
                {#each timedEventsOn(day.key) as event (event.id)}
                  {@const minutes = eventMinutes(event)}
                  <button
                    class="time-event"
                    style:top={`${(minutes.start / 60) * HOUR_HEIGHT}px`}
                    style:height={`${Math.max(24, ((minutes.end - minutes.start) / 60) * HOUR_HEIGHT)}px`}
                    style:border-left-color={eventColor(event)}
                    type="button"
                    onclick={() => (selectedEvent = event)}
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
      if (event.target === event.currentTarget) showCreate = false;
    }}>
      <form
        class="event-dialog"
        aria-label="New event"
        onsubmit={(event) => {
          event.preventDefault();
          void createEvent();
        }}
      >
        <div class="dialog-head">
          <div>
            <strong>New event</strong>
            <span>{formatDate(parseDayKey(draftEvent.date))}</span>
          </div>
          <button
            class="icon-button"
            type="button"
            aria-label="Close event editor"
            onclick={() => (showCreate = false)}
          >
            <X size={16} />
          </button>
        </div>
        <label class="wide">
          Title
          <input bind:value={draftEvent.summary} placeholder="Event title" required />
        </label>
        <label>
          Calendar
          <select bind:value={selectedCalendarId}>
            {#each calendars as calendar (calendar.id)}
              <option value={calendar.id}>{calendar.name}</option>
            {/each}
          </select>
        </label>
        <label>
          Date
          <input type="date" bind:value={draftEvent.date} required />
        </label>
        <label class="check-field">
          <input type="checkbox" bind:checked={draftEvent.allDay} />
          All day
        </label>
        {#if !draftEvent.allDay}
          <label>
            Starts
            <input type="time" bind:value={draftEvent.start} required />
          </label>
          <label>
            Ends
            <input type="time" bind:value={draftEvent.end} required />
          </label>
        {/if}
        <label>
          Location
          <input bind:value={draftEvent.location} placeholder="Optional" />
        </label>
        <label class="wide">
          Description
          <textarea bind:value={draftEvent.description} rows="3" placeholder="Optional"></textarea>
        </label>
        {#if createError}
          <p class="form-error" role="alert">{createError}</p>
        {/if}
        <div class="dialog-actions">
          <button type="button" onclick={() => (showCreate = false)}>Cancel</button>
          <button class="primary-button" type="submit" disabled={creating}>
            {creating ? "Saving…" : "Save event"}
          </button>
        </div>
      </form>
    </div>
  {/if}

  {#if selectedEvent}
    <div class="dialog-backdrop" role="presentation" onclick={(event) => {
      if (event.target === event.currentTarget) selectedEvent = null;
    }}>
      <section class="event-detail" aria-label="Event details">
        <div class="dialog-head">
          <div>
            <strong>{selectedEvent.summary}</strong>
            <span>{formatDate(eventDate(selectedEvent))} · {eventTimeLabel(selectedEvent)}</span>
          </div>
          <button
            class="icon-button"
            type="button"
            aria-label="Close event details"
            onclick={() => (selectedEvent = null)}
          >
            <X size={16} />
          </button>
        </div>
        <span class="detail-calendar">
          <i style:background={eventColor(selectedEvent)}></i>
          {selectedEvent.calendarName}
        </span>
        {#if selectedEvent.location}
          <p class="detail-line">{selectedEvent.location}</p>
        {/if}
        {#if selectedEvent.description}
          <p class="detail-description">{selectedEvent.description}</p>
        {/if}
        <div class="dialog-actions">
          <button
            class="danger-button"
            type="button"
            disabled={deletingId !== null}
            onclick={() => void deleteEvent(selectedEvent!)}
          >
            <Trash2 size={14} />
            {deletingId !== null ? "Deleting…" : "Delete"}
          </button>
          <button type="button" onclick={() => (selectedEvent = null)}>Close</button>
        </div>
      </section>
    </div>
  {/if}

  {#if showAddAccount}
    <CalendarAccountDialog
      spaceId={app.activeSpaceId ?? 1}
      onClose={() => (showAddAccount = false)}
      onAdded={accountAdded}
    />
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
    border-radius: 6px;
    background: var(--bg-raised);
    color: var(--text);
    padding: 6px 7px;
    font: inherit;
    font-size: 11px;
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

  .hour-lines {
    position: absolute;
    inset: 0;
    background: repeating-linear-gradient(
      to bottom,
      transparent 0,
      transparent calc(var(--hour-height, 56px) - 1px),
      var(--border) calc(var(--hour-height, 56px) - 1px),
      var(--border) var(--hour-height, 56px)
    );
    pointer-events: none;
  }

  .time-event {
    position: absolute;
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

  .event-dialog,
  .event-detail {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 10px;
    width: min(620px, 100%);
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--bg-raised);
    box-shadow: 0 18px 50px color-mix(in srgb, #000 35%, transparent);
  }

  .event-detail {
    display: flex;
    flex-direction: column;
    width: min(440px, 100%);
  }

  .dialog-head {
    grid-column: 1 / -1;
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
    font-size: 14px;
  }

  .dialog-head span {
    color: var(--muted);
    font-size: 11px;
  }

  .event-dialog label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--muted);
    font-size: 10px;
  }

  .event-dialog .wide,
  .form-error,
  .dialog-actions {
    grid-column: 1 / -1;
  }

  .event-dialog textarea {
    resize: vertical;
  }

  .event-dialog .check-field {
    flex-direction: row;
    align-items: center;
    align-self: end;
    padding-bottom: 7px;
    color: var(--text);
  }

  .event-dialog .check-field input {
    width: auto;
    accent-color: var(--accent);
  }

  .form-error {
    margin: 0;
    color: var(--danger);
    font-size: 11px;
  }

  .dialog-actions {
    justify-content: flex-end;
    margin-top: 2px;
  }

  .dialog-actions .danger-button {
    margin-right: auto;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--danger);
    border-color: color-mix(in srgb, var(--danger) 35%, var(--border));
  }

  .detail-calendar {
    gap: 6px;
    color: var(--muted);
    font-size: 11px;
  }

  .detail-calendar i {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }

  .detail-line,
  .detail-description {
    margin: 0;
    color: var(--text);
    font-size: 12px;
    line-height: 1.5;
    white-space: pre-wrap;
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
      grid-template-columns: 1fr;
    }

    .event-dialog .wide,
    .form-error,
    .dialog-actions {
      grid-column: 1;
    }
  }
</style>
