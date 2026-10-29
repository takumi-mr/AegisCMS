import { describe, expect, it } from 'vitest';
import { bufferReducer, dirtyBuffers, emptyBuffers, isDirty, type BufferState } from './buffers.ts';

const open = (state: BufferState, path: string, content = 'a') =>
  bufferReducer(state, { type: 'loaded', path, content });

describe('bufferReducer', () => {
  it('opens a file as the active tab, once', () => {
    let state = open(emptyBuffers, 'templates/a.html');
    state = open(state, 'templates/b.html');
    state = bufferReducer(state, { type: 'loaded', path: 'templates/a.html', content: 'x' });
    expect(state.open).toEqual(['templates/a.html', 'templates/b.html']);
    expect(state.active).toBe('templates/a.html');
  });

  it('tracks unsaved edits until they are saved', () => {
    let state = open(emptyBuffers, 'templates/a.html', 'one');
    state = bufferReducer(state, { type: 'edit', path: 'templates/a.html', content: 'two' });
    expect(isDirty(state.buffers['templates/a.html'])).toBe(true);
    expect(dirtyBuffers(state)).toHaveLength(1);
    state = bufferReducer(state, { type: 'saved', path: 'templates/a.html', content: 'two' });
    expect(dirtyBuffers(state)).toHaveLength(0);
  });

  it('ignores edits that change nothing and files that are not open', () => {
    const state = open(emptyBuffers, 'templates/a.html', 'one');
    expect(bufferReducer(state, { type: 'edit', path: 'templates/a.html', content: 'one' })).toBe(
      state
    );
    expect(bufferReducer(state, { type: 'edit', path: 'templates/zzz.html', content: 'x' })).toBe(
      state
    );
  });

  it('takes a change on disk when there are no edits, and flags a conflict when there are', () => {
    let state = open(emptyBuffers, 'templates/a.html', 'one');
    state = bufferReducer(state, { type: 'external', path: 'templates/a.html', content: 'disk' });
    expect(state.buffers['templates/a.html']).toMatchObject({ content: 'disk', conflict: false });

    state = bufferReducer(state, { type: 'edit', path: 'templates/a.html', content: 'mine' });
    state = bufferReducer(state, { type: 'external', path: 'templates/a.html', content: 'disk2' });
    expect(state.buffers['templates/a.html']).toMatchObject({ content: 'mine', conflict: true });

    // saving over it resolves the conflict
    state = bufferReducer(state, { type: 'saved', path: 'templates/a.html', content: 'mine' });
    expect(state.buffers['templates/a.html'].conflict).toBe(false);
  });

  it('does nothing when the disk content is what we already have (our own save)', () => {
    const state = open(emptyBuffers, 'templates/a.html', 'one');
    expect(
      bufferReducer(state, { type: 'external', path: 'templates/a.html', content: 'one' })
    ).toBe(state);
  });

  it('activates the neighbouring tab when the active one closes', () => {
    let state = open(emptyBuffers, 'a');
    state = open(state, 'b');
    state = open(state, 'c');
    state = bufferReducer(state, { type: 'activate', path: 'b' });
    state = bufferReducer(state, { type: 'close', path: 'b' });
    expect(state.open).toEqual(['a', 'c']);
    expect(state.active).toBe('c');
    expect(state.buffers.b).toBeUndefined();

    state = bufferReducer(state, { type: 'close', path: 'c' });
    expect(state.active).toBe('a');
    state = bufferReducer(state, { type: 'close', path: 'a' });
    expect(state.active).toBeNull();
  });
});
