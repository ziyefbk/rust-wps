// Browser-free real schema/plugin/history/transport doubles only.
export default {testDir: '.', testMatch: /(?:word-table-cell-colors-model|word-table-cell-no-fill-integration|word-table-row-pagination-(?:model|transport))\.spec\.mjs$/,
  workers: 1, timeout: 60000, outputDir: '../../tmp/table-cell-no-fill-model-results', reporter: 'list'};
