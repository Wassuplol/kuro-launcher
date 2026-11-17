export function greeting(now: Date = new Date()): string {
  const hour = now.getHours();
  if (hour < 5) return "Still up";
  if (hour < 12) return "Good morning";
  if (hour < 18) return "Good afternoon";
  return "Good evening";
}

/** "Horror, RPG and 2 more" without pretending to know what the list means. */
export function listNames(names: string[]): string {
  if (names.length === 0) return "";
  if (names.length === 1) return names[0] ?? "";
  if (names.length === 2) return `${names[0]} and ${names[1]}`;
  const rest = names.length - 2;
  return `${names[0]}, ${names[1]} and ${rest} more`;
}

/** Bytes as a person reads them. One decimal only where it changes the number. */
export function bytes(value: number): string {
  if (value <= 0) return "0 MB";
  const mb = value / 1_000_000;
  if (mb < 1) return `${Math.max(1, Math.round(value / 1000))} KB`;
  if (mb < 1000) return `${mb < 10 ? mb.toFixed(1) : Math.round(mb)} MB`;
  const gb = value / 1_000_000_000;
  return `${gb.toFixed(2)} GB`;
}

/** The clock time of a moment given in unix seconds. */
export function clock(unix: number): string {
  return new Date(unix * 1000).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** A short date, for "last touched" style metadata. */
export function day(unix: number): string {
  return new Date(unix * 1000).toLocaleDateString(undefined, {
    day: "numeric",
    month: "short",
  });
}
