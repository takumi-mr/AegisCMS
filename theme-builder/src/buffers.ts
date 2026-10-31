/** Editor buffers: what is open, what is edited, and what changed on disk meanwhile */

export interface Buffer {
  path: string;
  /** The content on disk as last read or written by this app */
  saved: string;
  content: string;
  /** The file changed on disk while it had unsaved edits */
  conflict: boolean;
}

export interface BufferState {
  buffers: Record<string, Buffer>;
  /** Tab order */
  open: string[];
  active: string | null;
}

export type BufferAction =
  | { type: 'loaded'; path: string; content: string }
  | { type: 'edit'; path: string; content: string }
  | { type: 'saved'; path: string; content: string }
  | { type: 'external'; path: string; content: string }
  | { type: 'activate'; path: string }
  | { type: 'close'; path: string }
  | { type: 'renamed'; from: string; to: string };

export const emptyBuffers: BufferState = { buffers: {}, open: [], active: null };

export function isDirty(buffer: Buffer): boolean {
  return buffer.content !== buffer.saved;
}

export function dirtyBuffers(state: BufferState): Buffer[] {
  return Object.values(state.buffers).filter(isDirty);
}

export function bufferReducer(state: BufferState, action: BufferAction): BufferState {
  switch (action.type) {
    case 'loaded': {
      const buffers = {
        ...state.buffers,
        [action.path]: {
          path: action.path,
          saved: action.content,
          content: action.content,
          conflict: false,
        },
      };
      const open = state.open.includes(action.path) ? state.open : [...state.open, action.path];
      return { buffers, open, active: action.path };
    }
    case 'edit': {
      const buffer = state.buffers[action.path];
      if (!buffer || buffer.content === action.content) return state;
      return {
        ...state,
        buffers: { ...state.buffers, [action.path]: { ...buffer, content: action.content } },
      };
    }
    case 'saved': {
      const buffer = state.buffers[action.path];
      if (!buffer) return state;
      return {
        ...state,
        buffers: {
          ...state.buffers,
          [action.path]: { ...buffer, saved: action.content, conflict: false },
        },
      };
    }
    case 'external': {
      const buffer = state.buffers[action.path];
      if (!buffer || buffer.saved === action.content) return state;
      const next = isDirty(buffer)
        ? { ...buffer, conflict: true }
        : { ...buffer, saved: action.content, content: action.content };
      return { ...state, buffers: { ...state.buffers, [action.path]: next } };
    }
    case 'activate':
      return state.open.includes(action.path) ? { ...state, active: action.path } : state;
    case 'close': {
      const open = state.open.filter((p) => p !== action.path);
      const { [action.path]: _closed, ...buffers } = state.buffers;
      let active = state.active;
      if (active === action.path) {
        const index = state.open.indexOf(action.path);
        active = open[Math.min(index, open.length - 1)] ?? null;
      }
      return { buffers, open, active };
    }
    case 'renamed': {
      const { from, to } = action;
      if (!state.buffers[from]) return state;
      const { [from]: oldBuffer, ...rest } = state.buffers;
      const buffers = {
        ...rest,
        [to]: { ...oldBuffer, path: to },
      };
      const open = state.open.map((p) => (p === from ? to : p));
      const active = state.active === from ? to : state.active;
      return { buffers, open, active };
    }
  }
}
