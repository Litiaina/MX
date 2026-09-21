import { describe, expect, it } from 'vitest';
import { secondFactor } from './auth';

describe('secondFactor', () => {
  it('classifies a six-digit value as a TOTP code', () => {
    expect(secondFactor(' 123456 ')).toEqual({
      otp: '123456',
      recovery_code: null
    });
  });

  it('classifies formatted values as recovery codes', () => {
    expect(secondFactor('ABCD-EFGH-IJKL-MNOP-QRST')).toEqual({
      otp: null,
      recovery_code: 'ABCD-EFGH-IJKL-MNOP-QRST'
    });
  });

  it('omits an empty second factor', () => {
    expect(secondFactor('  ')).toEqual({ otp: null, recovery_code: null });
  });
});
