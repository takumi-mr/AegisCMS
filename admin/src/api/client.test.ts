import { describe, it, expect } from 'vitest';
import { ApiError } from './client';

describe('ApiError class', () => {
  it('constructs with status and message', () => {
    const err = new ApiError(404, 'Not Found');
    expect(err).toBeInstanceOf(Error);
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(404);
    expect(err.message).toBe('Not Found');
    expect(err.retryAfterSecs).toBeUndefined();
    expect(err.code).toBeUndefined();
  });

  it('preserves optional retryAfterSecs and error code', () => {
    const err = new ApiError(429, 'Too Many Requests', 60, 'rate_limited');
    expect(err.status).toBe(429);
    expect(err.message).toBe('Too Many Requests');
    expect(err.retryAfterSecs).toBe(60);
    expect(err.code).toBe('rate_limited');
  });
});
