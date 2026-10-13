// No browser fixtures or web server: model/plugin/DOM declarations only.
export default {testDir: '.', testMatch: /word-(?:empty-paragraph-font-model|block-type-model|block-type-interactions|block-type-upstream|contextual-spacing-model)\.spec\.mjs$/, workers: 1, timeout: 60000,
  outputDir: '../../tmp/empty-paragraph-font-model-results', reporter: 'list'};
