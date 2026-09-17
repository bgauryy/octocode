'use strict'
const { getPlatformSuffix } = require('./bin/platform.cjs')
const suffix = getPlatformSuffix()
if (!suffix) throw new Error(`@octocodeai/octocode-native does not ship an addon for ${process.platform}-${process.arch}`)
module.exports = require(`@octocodeai/octocode-native-${suffix}`)
