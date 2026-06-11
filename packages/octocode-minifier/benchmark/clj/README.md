# Clojure (.clj)

Source sample: `clj/clojure-core.clj`

Strategy: `aggressive`

Agent rating: **7.8/10 (good)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 276207 | - | - | - |
| content-view | 274661 | 0.6% | 30.15 ms | 7.8/10 |
| applyMinification | 229282 | 17% | 22.236 ms | 7.8/10 |
| sync minify | 229282 | 17% | 24.101 ms | 7.8/10 |
| async minify | 229282 | 17% | 22.187 ms | 7.8/10 |
| symbols | n/a | n/a | 0.016 ms | n/a |

## Notes

- aggressive text strategy.
- symbols are not implemented for this extension.

## Before Excerpt

```clj
;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(ns ^{:doc "The core Clojure language."
       :author "Rich Hickey"}
  clojure.core)

(def unquote)
(def unquote-splicing)

(def
 ^{:arglists '([& items])
   :doc "Creates a new list containing the items."
   :added "1.0"}
  list (. clojure.lang.PersistentList creator))

(def
 ^{:arglists '([x seq])
    :doc "Returns a new seq where x is the first element and seq is
    the rest."
   :added "1.0"
   :static true}

 cons (fn* ^:static cons [x seq] (. clojure.lang.RT (cons x seq))))

;during bootstrap we don't have destructuring let, loop or fn, will redefine later
(def
  ^{:macro true
    :added "1.0"}
  let (fn* let [&form &env & decl] (cons 'let* decl)))

(def
 ^{:macro true
   :added "1.0"}
 loop (fn* loop [&form &env & decl] (cons 'loop* decl)))


... [truncated 274407 chars] ...

:added "1.11"}
  [^String s]
  (if (string? s)
    (case s
      "true" true
      "false" false
      nil)
    (throw (IllegalArgumentException. (parsing-err s)))))

(defn NaN?
  {:doc "Returns true if num is NaN, else false"
   :inline-arities #{1}
   :inline (fn [num] `(Double/isNaN ~num))
   :added "1.11"}

  [^double num]
  (Double/isNaN num))

(defn infinite?
  {:doc "Returns true if num is negative or positive infinity, else false"
   :inline-arities #{1}
   :inline (fn [num] `(Double/isInfinite ~num))
   :added "1.11"}
  [^double num]
  (Double/isInfinite num))

```

## Content-View Excerpt

```clj
(ns ^{:doc "The core Clojure language."
       :author "Rich Hickey"}
  clojure.core)

(def unquote)
(def unquote-splicing)

(def
 ^{:arglists '([& items])
   :doc "Creates a new list containing the items."
   :added "1.0"}
  list (. clojure.lang.PersistentList creator))

(def
 ^{:arglists '([x seq])
    :doc "Returns a new seq where x is the first element and seq is
    the rest."
   :added "1.0"
   :static true}

 cons (fn* ^:static cons [x seq] (. clojure.lang.RT (cons x seq))))

(def
  ^{:macro true
    :added "1.0"}
  let (fn* let [&form &env & decl] (cons 'let* decl)))

(def
 ^{:macro true
   :added "1.0"}
 loop (fn* loop [&form &env & decl] (cons 'loop* decl)))

(def
 ^{:macro true
   :added "1.0"}
 fn (fn* fn [&form &env & decl]
         (.withMeta ^clojure.lang.IObj (cons 'fn* decl)
                    (.meta ^clojure.lang.IMeta &form))))

(def
 ^{:arglists '([coll])
   :doc "Returns the first item in the collection. Calls seq on its
    argument. If coll is nil, returns nil."
   :added "1.0"
   :static true}
 first (fn ^:static first [coll] (. clojure.lang.RT (first coll))))

(def
 ^{:arglists '([coll])
   :tag clojure.lang.ISeq
   :doc "Returns a seq of the items after the first. Calls seq on i

... [truncated 272861 chars] ...

 :added "1.11"}
  [^String s]
  (if (string? s)
    (case s
      "true" true
      "false" false
      nil)
    (throw (IllegalArgumentException. (parsing-err s)))))

(defn NaN?
  {:doc "Returns true if num is NaN, else false"
   :inline-arities #{1}
   :inline (fn [num] `(Double/isNaN ~num))
   :added "1.11"}

  [^double num]
  (Double/isNaN num))

(defn infinite?
  {:doc "Returns true if num is negative or positive infinity, else false"
   :inline-arities #{1}
   :inline (fn [num] `(Double/isInfinite ~num))
   :added "1.11"}
  [^double num]
  (Double/isInfinite num))
```

## Apply Minification Excerpt

```clj
(ns ^{:doc "The core Clojure language.":author "Rich Hickey"}clojure.core) (def unquote) (def unquote-splicing) (def ^{:arglists '([& items]):doc "Creates a new list containing the items.":added "1.0"}list (. clojure.lang.PersistentList creator)) (def ^{:arglists '([x seq]):doc "Returns a new seq where x is the first element and seq is the rest.":added "1.0":static true}cons (fn* ^:static cons [x seq] (. clojure.lang.RT (cons x seq)))) (def ^{:macro true:added "1.0"}let (fn* let [&form &env & decl] (cons 'let* decl))) (def ^{:macro true:added "1.0"}loop (fn* loop [&form &env & decl] (cons 'loop* decl))) (def ^{:macro true:added "1.0"}fn (fn* fn [&form &env & decl] (.withMeta ^clojure.lang.IObj (cons 'fn* decl) (.meta ^clojure.lang.IMeta &form)))) (def ^{:arglists '([coll]):doc "Returns the first item in the collection. Calls seq on its argument. If coll is nil,returns nil.":added "1.0":static true}first (fn ^:static first [coll] (. clojure.lang.RT (first coll)))) (def ^{:arglists '([coll]):tag clojure.lang.ISeq:doc "Returns a seq of the items after the first. Calls seq on its argument. If there are no more items,returns nil.":added "1.0":static true}next (fn ^:static next [x] (. clojure.lang.RT (next x)))

... [truncated 227482 chars] ...

 "Parse strings \"true\" or \"false\" and return a boolean,or nil if invalid":added "1.11"}[^String s] (if (string? s) (case s "true" true "false" false nil) (throw (IllegalArgumentException. (parsing-err s))))) (defn NaN?{:doc "Returns true if num is NaN,else false":inline-arities #{1}:inline (fn [num] `(Double/isNaN ~num)):added "1.11"}[^double num] (Double/isNaN num)) (defn infinite?{:doc "Returns true if num is negative or positive infinity,else false":inline-arities #{1}:inline (fn [num] `(Double/isInfinite ~num)):added "1.11"}[^double num] (Double/isInfinite num))
```

## Sync Minify Excerpt

```clj
(ns ^{:doc "The core Clojure language.":author "Rich Hickey"}clojure.core) (def unquote) (def unquote-splicing) (def ^{:arglists '([& items]):doc "Creates a new list containing the items.":added "1.0"}list (. clojure.lang.PersistentList creator)) (def ^{:arglists '([x seq]):doc "Returns a new seq where x is the first element and seq is the rest.":added "1.0":static true}cons (fn* ^:static cons [x seq] (. clojure.lang.RT (cons x seq)))) (def ^{:macro true:added "1.0"}let (fn* let [&form &env & decl] (cons 'let* decl))) (def ^{:macro true:added "1.0"}loop (fn* loop [&form &env & decl] (cons 'loop* decl))) (def ^{:macro true:added "1.0"}fn (fn* fn [&form &env & decl] (.withMeta ^clojure.lang.IObj (cons 'fn* decl) (.meta ^clojure.lang.IMeta &form)))) (def ^{:arglists '([coll]):doc "Returns the first item in the collection. Calls seq on its argument. If coll is nil,returns nil.":added "1.0":static true}first (fn ^:static first [coll] (. clojure.lang.RT (first coll)))) (def ^{:arglists '([coll]):tag clojure.lang.ISeq:doc "Returns a seq of the items after the first. Calls seq on its argument. If there are no more items,returns nil.":added "1.0":static true}next (fn ^:static next [x] (. clojure.lang.RT (next x)))

... [truncated 227482 chars] ...

 "Parse strings \"true\" or \"false\" and return a boolean,or nil if invalid":added "1.11"}[^String s] (if (string? s) (case s "true" true "false" false nil) (throw (IllegalArgumentException. (parsing-err s))))) (defn NaN?{:doc "Returns true if num is NaN,else false":inline-arities #{1}:inline (fn [num] `(Double/isNaN ~num)):added "1.11"}[^double num] (Double/isNaN num)) (defn infinite?{:doc "Returns true if num is negative or positive infinity,else false":inline-arities #{1}:inline (fn [num] `(Double/isInfinite ~num)):added "1.11"}[^double num] (Double/isInfinite num))
```

## Async Minify Excerpt

```clj
(ns ^{:doc "The core Clojure language.":author "Rich Hickey"}clojure.core) (def unquote) (def unquote-splicing) (def ^{:arglists '([& items]):doc "Creates a new list containing the items.":added "1.0"}list (. clojure.lang.PersistentList creator)) (def ^{:arglists '([x seq]):doc "Returns a new seq where x is the first element and seq is the rest.":added "1.0":static true}cons (fn* ^:static cons [x seq] (. clojure.lang.RT (cons x seq)))) (def ^{:macro true:added "1.0"}let (fn* let [&form &env & decl] (cons 'let* decl))) (def ^{:macro true:added "1.0"}loop (fn* loop [&form &env & decl] (cons 'loop* decl))) (def ^{:macro true:added "1.0"}fn (fn* fn [&form &env & decl] (.withMeta ^clojure.lang.IObj (cons 'fn* decl) (.meta ^clojure.lang.IMeta &form)))) (def ^{:arglists '([coll]):doc "Returns the first item in the collection. Calls seq on its argument. If coll is nil,returns nil.":added "1.0":static true}first (fn ^:static first [coll] (. clojure.lang.RT (first coll)))) (def ^{:arglists '([coll]):tag clojure.lang.ISeq:doc "Returns a seq of the items after the first. Calls seq on its argument. If there are no more items,returns nil.":added "1.0":static true}next (fn ^:static next [x] (. clojure.lang.RT (next x)))

... [truncated 227482 chars] ...

 "Parse strings \"true\" or \"false\" and return a boolean,or nil if invalid":added "1.11"}[^String s] (if (string? s) (case s "true" true "false" false nil) (throw (IllegalArgumentException. (parsing-err s))))) (defn NaN?{:doc "Returns true if num is NaN,else false":inline-arities #{1}:inline (fn [num] `(Double/isNaN ~num)):added "1.11"}[^double num] (Double/isNaN num)) (defn infinite?{:doc "Returns true if num is negative or positive infinity,else false":inline-arities #{1}:inline (fn [num] `(Double/isInfinite ~num)):added "1.11"}[^double num] (Double/isInfinite num))
```

## Symbols

```txt
No symbols returned for this sample.
```
