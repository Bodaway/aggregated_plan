const { TsJestTransformer } = require('ts-jest');

const tsJestInstance = new TsJestTransformer();

module.exports = {
  canInstrument: true,
  getCacheKey(sourceText, sourcePath, options) {
    return tsJestInstance.getCacheKey(sourceText, sourcePath, options);
  },
  process(sourceText, sourcePath, options) {
    const replaced = sourceText.replace(
      /import\.meta\.env\.(\w+)/g,
      'process.env.$1',
    );
    return tsJestInstance.process(replaced, sourcePath, options);
  },
};
