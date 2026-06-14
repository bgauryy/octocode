# Ruby (.rb)

Source sample: `rb/blank.rb`

Strategy: `conservative`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 3507 | - | - |
| content-view | 1254 | 64.2% | 0.067 ms |
| applyMinification | 1269 | 63.8% | 0.063 ms |
| sync minify | 1269 | 63.8% | 0.069 ms |
| async minify | 1269 | 63.8% | 0.08 ms |
| symbols | 650 | 81.5% | 0.236 ms |

## Notes

- conservative text strategy.

## Before Excerpt

```rb
# frozen_string_literal: true

require "concurrent/map"

class Object
  # An object is blank if it's false, empty, or a whitespace string.
  # For example, +nil+, '', '   ', [], {}, and +false+ are all blank.
  #
  # This simplifies
  #
  #   !address || address.empty?
  #
  # to
  #
  #   address.blank?
  #
  # @return [true, false]
  def blank?
    respond_to?(:empty?) ? !!empty? : false
  end

  # An object is present if it's not blank.
  #
  # @return [true, false]
  def present?
    !blank?
  end

  # Returns the receiver if it's present otherwise returns +nil+.
  # <tt>object.presence</tt> is equivalent to
  #
  #    object.present? ? object : nil
  #
  # For example, something like
  #
  #   state   = params[:state]   if params[:state].present?
  #   country = params[:country] if params[:country].present?
  #   region  = state || country || 'US'
  #
  # becomes
  #
  #   region = params[:state].presence || params[:country].presence || 'US'
  #
  # @return [Object]
  def presence
    self if present?
  end
end

class NilClass
  # +nil+ is blank:
  #
  #   nil.blank? # => true
  #
  # @return [true]
  def blank?
    true
  end

  def present? # :nodoc:
    false
  end
end

class FalseClass
  # +false

... [truncated 1707 chars] ...


    empty? ||
      begin
        BLANK_RE.match?(self)
      rescue Encoding::CompatibilityError
        ENCODED_BLANKS[self.encoding].match?(self)
      end
  end

  def present? # :nodoc:
    !blank?
  end
end

class Numeric # :nodoc:
  # No number is blank:
  #
  #   1.blank? # => false
  #   0.blank? # => false
  #
  # @return [false]
  def blank?
    false
  end

  def present?
    true
  end
end

class Time # :nodoc:
  # No Time is blank:
  #
  #   Time.now.blank? # => false
  #
  # @return [false]
  def blank?
    false
  end

  def present?
    true
  end
end

```

## Content-View Excerpt

```rb
require "concurrent/map"

class Object

  def blank?
    respond_to?(:empty?) ? !!empty? : false
  end

  def present?
    !blank?
  end

  def presence
    self if present?
  end
end

class NilClass

  def blank?
    true
  end

  def present?
    false
  end
end

class FalseClass

  def blank?
    true
  end

  def present?
    false
  end
end

class TrueClass

  def blank?
    false
  end

  def present?
    true
  end
end

class Array

  alias_method :blank?, :empty?

  def present?
    !empty?
  end
end

class Hash

  alias_method :blank?, :empty?

  def present?
    !empty?
  end
end

class Symbol

  alias_method :blank?, :empty?

  def present?
    !empty?
  end
end

class String
  BLANK_RE = /\A[[:space:]]*\z/
  ENCODED_BLANKS = Concurrent::Map.new do |h, enc|
    h[enc] = Regexp.new(BLANK_RE.source.encode(enc), BLANK_RE.options | Regexp::FIXEDENCODING)
  end

  def blank?

    empty? ||
      begin
        BLANK_RE.match?(self)
      rescue Encoding::CompatibilityError
        ENCODED_BLANKS[self.encoding].match?(self)
      end
  end

  def present?
    !blank?
  end
end

class Numeric

  def blank?
    false
  end

  def present?
    true
  end
end

class Time

  def blank?
    false
  end

  def present?
    true
  end
end
```

## Apply Minification Excerpt

```rb


require "concurrent/map"

class Object


  def blank?
    respond_to?(:empty?) ? !!empty? : false
  end


  def present?
    !blank?
  end


  def presence
    self if present?
  end
end

class NilClass


  def blank?
    true
  end

  def present?
    false
  end
end

class FalseClass


  def blank?
    true
  end

  def present?
    false
  end
end

class TrueClass


  def blank?
    false
  end

  def present?
    true
  end
end

class Array


  alias_method :blank?, :empty?

  def present?
    !empty?
  end
end

class Hash


  alias_method :blank?, :empty?

  def present?
    !empty?
  end
end

class Symbol


  alias_method :blank?, :empty?

  def present?
    !empty?
  end
end

class String
  BLANK_RE = /\A[[:space:]]*\z/
  ENCODED_BLANKS = Concurrent::Map.new do |h, enc|
    h[enc] = Regexp.new(BLANK_RE.source.encode(enc), BLANK_RE.options | Regexp::FIXEDENCODING)
  end


  def blank?


    empty? ||
      begin
        BLANK_RE.match?(self)
      rescue Encoding::CompatibilityError
        ENCODED_BLANKS[self.encoding].match?(self)
      end
  end

  def present?
    !blank?
  end
end

class Numeric


  def blank?
    false
  end

  def present?
    true
  end
end

class Time


  def blank?
    false
  end

  def present?
    true
  end
end
```

## Sync Minify Excerpt

```rb


require "concurrent/map"

class Object


  def blank?
    respond_to?(:empty?) ? !!empty? : false
  end


  def present?
    !blank?
  end


  def presence
    self if present?
  end
end

class NilClass


  def blank?
    true
  end

  def present?
    false
  end
end

class FalseClass


  def blank?
    true
  end

  def present?
    false
  end
end

class TrueClass


  def blank?
    false
  end

  def present?
    true
  end
end

class Array


  alias_method :blank?, :empty?

  def present?
    !empty?
  end
end

class Hash


  alias_method :blank?, :empty?

  def present?
    !empty?
  end
end

class Symbol


  alias_method :blank?, :empty?

  def present?
    !empty?
  end
end

class String
  BLANK_RE = /\A[[:space:]]*\z/
  ENCODED_BLANKS = Concurrent::Map.new do |h, enc|
    h[enc] = Regexp.new(BLANK_RE.source.encode(enc), BLANK_RE.options | Regexp::FIXEDENCODING)
  end


  def blank?


    empty? ||
      begin
        BLANK_RE.match?(self)
      rescue Encoding::CompatibilityError
        ENCODED_BLANKS[self.encoding].match?(self)
      end
  end

  def present?
    !blank?
  end
end

class Numeric


  def blank?
    false
  end

  def present?
    true
  end
end

class Time


  def blank?
    false
  end

  def present?
    true
  end
end
```

## Async Minify Excerpt

```rb


require "concurrent/map"

class Object


  def blank?
    respond_to?(:empty?) ? !!empty? : false
  end


  def present?
    !blank?
  end


  def presence
    self if present?
  end
end

class NilClass


  def blank?
    true
  end

  def present?
    false
  end
end

class FalseClass


  def blank?
    true
  end

  def present?
    false
  end
end

class TrueClass


  def blank?
    false
  end

  def present?
    true
  end
end

class Array


  alias_method :blank?, :empty?

  def present?
    !empty?
  end
end

class Hash


  alias_method :blank?, :empty?

  def present?
    !empty?
  end
end

class Symbol


  alias_method :blank?, :empty?

  def present?
    !empty?
  end
end

class String
  BLANK_RE = /\A[[:space:]]*\z/
  ENCODED_BLANKS = Concurrent::Map.new do |h, enc|
    h[enc] = Regexp.new(BLANK_RE.source.encode(enc), BLANK_RE.options | Regexp::FIXEDENCODING)
  end


  def blank?


    empty? ||
      begin
        BLANK_RE.match?(self)
      rescue Encoding::CompatibilityError
        ENCODED_BLANKS[self.encoding].match?(self)
      end
  end

  def present?
    !blank?
  end
end

class Numeric


  def blank?
    false
  end

  def present?
    true
  end
end

class Time


  def blank?
    false
  end

  def present?
    true
  end
end
```

## Symbols

```txt
  3| require "concurrent/map"
  5| class Object
 18|   def blank?
 25|   def present?
 45|   def presence
 50| class NilClass
 56|   def blank?
 60|   def present? # :nodoc:
 65| class FalseClass
 71|   def blank?
 75|   def present? # :nodoc:
 80| class TrueClass
 86|   def blank?
 90|   def present? # :nodoc:
 95| class Array
104|   def present? # :nodoc:
109| class Hash
118|   def present? # :nodoc:
123| class Symbol
130|   def present? # :nodoc:
135| class String
153|   def blank?
165|   def present? # :nodoc:
170| class Numeric # :nodoc:
177|   def blank?
181|   def present?
186| class Time # :nodoc:
192|   def blank?
196|   def present?
```
