import nativeBinding from './index.js'

export const SIGNATURES_ONLY_HINT = nativeBinding.SIGNATURES_ONLY_HINT
export const getExtension = nativeBinding.getExtension
export const minifyContentSync = nativeBinding.minifyContentSync
export const minifyContentResult = nativeBinding.minifyContentResult
export const minifyContent = nativeBinding.minifyContent
export const applyMinification = nativeBinding.applyMinification
export const applyContentViewMinification = nativeBinding.applyContentViewMinification
export const removeComments = nativeBinding.removeComments
export const minifyConservativeCore = nativeBinding.minifyConservativeCore
export const minifyAggressiveCore = nativeBinding.minifyAggressiveCore
export const minifyJsonCore = nativeBinding.minifyJsonCore
export const minifyJsonReadable = nativeBinding.minifyJsonReadable
export const minifyCodeCore = nativeBinding.minifyCodeCore
export const minifyGeneralCore = nativeBinding.minifyGeneralCore
export const minifyMarkdownCore = nativeBinding.minifyMarkdownCore
export const minifyCSSCore = nativeBinding.minifyCSSCore
export const minifyHTMLCore = nativeBinding.minifyHTMLCore
export const minifyJavaScriptCore = nativeBinding.minifyJavaScriptCore
export const minifyCSSQuality = nativeBinding.minifyCSSQuality
export const minifyHTMLQuality = nativeBinding.minifyHTMLQuality
export const stripPythonDocstrings = nativeBinding.stripPythonDocstrings
export const extractSignatures = nativeBinding.extractSignatures
export const structuralSearch = nativeBinding.structuralSearch
export const getSemanticBoundaryOffsets = nativeBinding.getSemanticBoundaryOffsets
export const getSupportedSignatureExtensions = nativeBinding.getSupportedSignatureExtensions
export const jsonToYamlString = nativeBinding.jsonToYamlString
export const getMINIFY_CONFIG = nativeBinding.getMINIFY_CONFIG
export const MINIFY_CONFIG = nativeBinding.MINIFY_CONFIG
export const SUPPORTED_SIGNATURE_EXTENSIONS = nativeBinding.SUPPORTED_SIGNATURE_EXTENSIONS
export const parseRipgrepJson = nativeBinding.parseRipgrepJson
export const queryFileSystem = nativeBinding.queryFileSystem
export const charToByteOffset = nativeBinding.charToByteOffset
export const byteToCharOffset = nativeBinding.byteToCharOffset
export const byteSliceContent = nativeBinding.byteSliceContent
export const sliceContent = nativeBinding.sliceContent
export const extractMatchingLines = nativeBinding.extractMatchingLines
export const filterPatch = nativeBinding.filterPatch

export default nativeBinding
