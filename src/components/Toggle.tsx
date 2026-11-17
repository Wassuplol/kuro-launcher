import * as Switch from "@radix-ui/react-switch";

import { cn } from "../lib/cn";

type Props = {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
};

/** The one toggle in the app. Gold when on, because gold means live state. */
export function Toggle({ checked, onCheckedChange, label, disabled }: Props) {
  return (
    <Switch.Root
      checked={checked}
      onCheckedChange={onCheckedChange}
      disabled={disabled}
      aria-label={label}
      className={cn(
        "ui relative h-5 w-9 shrink-0 rounded-full border",
        checked ? "border-gold/50 bg-gold/90" : "border-white/12 bg-raised",
        disabled && "opacity-50",
      )}
    >
      <Switch.Thumb
        className={cn(
          "ui block h-3.5 w-3.5 rounded-full shadow-sm",
          checked ? "translate-x-[19px] bg-page" : "translate-x-[3px] bg-muted",
        )}
      />
    </Switch.Root>
  );
}
