const TOKEN_STORAGE_KEY = 'aegis.sessionToken';

// sessionStorage can be unavailable (private mode, blocked storage); the token then lives
// only in memory for this page load
let memoryToken = '';

/** Session token (`ses_...`) sent as `Authorization: Bearer` with every API request */
export const getSessionToken = (): string => {
  try {
    return sessionStorage.getItem(TOKEN_STORAGE_KEY) ?? memoryToken;
  } catch {
    return memoryToken;
  }
};

export const setSessionToken = (token: string) => {
  memoryToken = token;
  try {
    if (token) sessionStorage.setItem(TOKEN_STORAGE_KEY, token);
    else sessionStorage.removeItem(TOKEN_STORAGE_KEY);
  } catch {
    // ignore
  }
};
