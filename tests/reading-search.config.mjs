// Pure module tests: no web server, browser launch, or native desktop needed.
export default {
  testDir: './ui', testMatch: 'reading-search.spec.mjs', timeout: 30000,
  workers: 1, reporter: 'list', outputDir: '../tmp/reading-search-results',
};
