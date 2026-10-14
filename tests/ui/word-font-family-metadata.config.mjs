// Pure model/real plugin/DOM transport checks; does not launch a browser.
export default {testDir: '.', testMatch: /word-font-family-metadata-model\.spec\.mjs$/, workers: 1, timeout: 60000,
  outputDir: '../../tmp/font-family-metadata-model-results', reporter: 'list'};
