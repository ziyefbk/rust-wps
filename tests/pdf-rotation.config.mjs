// Actual frontend function tests; no server, browser, or native desktop launch.
export default {
  testDir: './ui', testMatch: ['pdf-rotation.spec.mjs', 'pdf-session-identity.spec.mjs', 'pdf-text.spec.mjs'], timeout: 30000,
  workers: 1, reporter: 'list', outputDir: '../tmp/pdf-rotation-ui-results',
};
