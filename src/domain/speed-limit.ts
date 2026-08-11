export const SPEED_UNITS = ["KB", "MB", "GB"] as const;
export type SpeedUnit = (typeof SPEED_UNITS)[number];

export interface SpeedLimitInputValue {
  amount: number;
  unit: SpeedUnit;
}

export const MAX_SPEED_LIMIT_BYTES = 10 * 1024 ** 3;

const BYTES_PER_UNIT: Readonly<Record<SpeedUnit, number>> = {
  KB: 1024,
  MB: 1024 ** 2,
  GB: 1024 ** 3,
};

export function isSpeedUnit(value: string): value is SpeedUnit {
  return SPEED_UNITS.some((unit) => unit === value);
}

export function speedLimitBytes(amount: number, unit: SpeedUnit): number {
  if (!Number.isFinite(amount) || amount <= 0) return 0;
  return Math.min(MAX_SPEED_LIMIT_BYTES, Math.round(amount * BYTES_PER_UNIT[unit]));
}

export function speedLimitInput(bytes: number, preferredUnit?: SpeedUnit): SpeedLimitInputValue {
  const normalized = Math.min(MAX_SPEED_LIMIT_BYTES, Math.max(0, Math.round(bytes)));
  const unit = preferredUnit ?? unitForBytes(normalized);
  return {
    amount: normalized === 0 ? 0 : roundedAmount(normalized / BYTES_PER_UNIT[unit]),
    unit,
  };
}

export function maxAmountForUnit(unit: SpeedUnit): number {
  return MAX_SPEED_LIMIT_BYTES / BYTES_PER_UNIT[unit];
}

function unitForBytes(bytes: number): SpeedUnit {
  if (bytes === 0) return "MB";
  if (bytes >= BYTES_PER_UNIT.GB) return "GB";
  if (bytes >= BYTES_PER_UNIT.MB) return "MB";
  return "KB";
}

function roundedAmount(value: number): number {
  return Number(value.toFixed(3));
}
