// Pure module tests: no web server, browser launch, or native desktop needed.
export default {
  testDir: './ui', testMatch: 'pdf-text.spec.mjs', timeout: 30000,
  workers: 1, reporter: 'list', outputDir: '../tmp/pdf-text-results',
};
