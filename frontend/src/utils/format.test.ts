import { describe, expect, it } from 'vitest';
import { formatCost, formatInt, formatPercent, formatTokens } from './format';

describe('formatTokens', () => {
  it('formats each K/M/B tier and the plain-integer base case', () => {
    expect(formatTokens(842)).toBe('842');
    expect(formatTokens(1_200)).toBe('1.2K');
    expect(formatTokens(3_500_000)).toBe('3.5M');
    expect(formatTokens(2_100_000_000)).toBe('2.1B');
  });

  it('returns 0 for non-positive or non-finite input', () => {
    expect(formatTokens(0)).toBe('0');
    expect(formatTokens(-5)).toBe('0');
    expect(formatTokens(Number.NaN)).toBe('0');
  });
});

describe('formatCost', () => {
  it('uses 2 decimals at $1+ and 4 decimals below', () => {
    expect(formatCost(12.345)).toBe('$12.35');
    expect(formatCost(0.1234)).toBe('$0.1234');
  });

  it('returns em dash for non-positive values', () => {
    expect(formatCost(0)).toBe('—');
    expect(formatCost(-1)).toBe('—');
  });
});

describe('formatPercent', () => {
  it('formats a ratio with the requested digits', () => {
    expect(formatPercent(0.1234)).toBe('12.3%');
    expect(formatPercent(0.5, 0)).toBe('50%');
    expect(formatPercent(Number.NaN)).toBe('—');
  });
});

describe('formatInt', () => {
  it('rounds and keeps exact values without K/M/B tiers', () => {
    expect(formatInt(1234567.4)).toBe((1234567).toLocaleString());
    expect(formatInt(Number.NaN)).toBe('0');
  });
});
