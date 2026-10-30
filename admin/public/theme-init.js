// Applies the color theme before the first paint, so a light theme does not flash dark.
// Keep the rules in step with src/theme/index.ts (a unit test runs this file against them).
(function () {
  try {
    var mode = null;
    try {
      mode = localStorage.getItem('aegis.admin.theme');
    } catch {}
    var light =
      mode === 'light' ||
      (mode !== 'dark' &&
        typeof window.matchMedia === 'function' &&
        window.matchMedia('(prefers-color-scheme: light)').matches);
    document.documentElement.setAttribute('data-theme', light ? 'light' : 'dark');
  } catch {}
})();
