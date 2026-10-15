// Actual model/editor/DOM transport checks, not browser or native acceptance.
export default {testDir: './ui', testMatch: ['word-minimum-line-height-model.spec.mjs', 'word-minimum-line-height-transport.spec.mjs'],
  timeout: 60000, workers: 1, reporter: 'list', outputDir: '../tmp/minimum-line-height-results'};
