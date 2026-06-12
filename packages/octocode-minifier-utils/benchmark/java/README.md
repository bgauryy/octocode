# Java (.java)

Source sample: `java/00-spring-annotation-utils.java`

Strategy: `conservative`

Agent rating: **9.7/10 (excellent)**

Agent understanding from minified output: **9.8/10 (excellent)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 63265 | - | - | - |
| content-view | 22271 | 64.8% | 7.027 ms | 9.5/10 |
| applyMinification | 22331 | 64.7% | 6.823 ms | 9.5/10 |
| sync minify | 22331 | 64.7% | 7.002 ms | 9.5/10 |
| async minify | 22331 | 64.7% | 6.683 ms | 9.5/10 |
| symbols | 8057 | 87.3% | 10.788 ms | 10/10 |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 10/10 (3/3) |
| delimiter structure | 10/10 |
| output health | 9/10 |
| context budget | 10/10 |
| symbol context | 10/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 63265 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 22271 | 64.8% | 9.8/10 excellent | 10/10 | 10/10 |
| minify | 22331 | 64.7% | 9.8/10 excellent | 10/10 | 10/10 |
| symbols | 8057 | 87.3% | 7.8/10 good | 6.7/10 | 6.7/10 |

## Notes

- conservative text strategy.

## Before Excerpt

```java
/*
 * Copyright 2002-present the original author or authors.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      https://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

package org.springframework.core.annotation;

import java.lang.annotation.Annotation;
import java.lang.reflect.AnnotatedElement;
import java.lang.reflect.Array;
import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.lang.reflect.Proxy;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.NoSuchElementException;
import java.util.Set;

import org.jspecify.annotations.Nullable;

import org.springframework.core.BridgeMethodResolver;
import org.springframework.c

... [truncated 61465 chars] ...

ation metadata cache.
	 * @since 4.3.15
	 */
	public static void clearCache() {
		AnnotationTypeMappings.clearCache();
		AnnotationsScanner.clearCache();
		AttributeMethods.cache.clear();
		RepeatableContainers.cache.clear();
		OrderUtils.orderCache.clear();
	}


	/**
	 * Internal holder used to wrap default values.
	 */
	private static class DefaultValueHolder {

		final Object defaultValue;

		public DefaultValueHolder(Object defaultValue) {
			this.defaultValue = defaultValue;
		}

		@Override
		public String toString() {
			return "*" + this.defaultValue;
		}
	}

}

```

## Content-View Excerpt

```java
package org.springframework.core.annotation;

import java.lang.annotation.Annotation;
import java.lang.reflect.AnnotatedElement;
import java.lang.reflect.Array;
import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.lang.reflect.Proxy;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.NoSuchElementException;
import java.util.Set;

import org.jspecify.annotations.Nullable;

import org.springframework.core.BridgeMethodResolver;
import org.springframework.core.annotation.AnnotationTypeMapping.MirrorSets.MirrorSet;
import org.springframework.core.annotation.MergedAnnotation.Adapt;
import org.springframework.core.annotation.MergedAnnotations.SearchStrategy;
import org.springframework.util.ClassUtils;
import org.springframework.util.CollectionUtils;
import org.springframework.util.ConcurrentReferenceHashMap;
import org.springframework.util.ReflectionUtils;
import org.springframework.util.StringUtils;

public abstract class AnnotationUtils {

	public static final String VALUE = MergedAnnotation.VALUE;

	private static final AnnotationFilter JAVA_LANG_ANNOTATION_FILTER =
			A

... [truncated 20471 chars] ...

hesizedMergedAnnotationInvocationHandler);
		}
		catch (SecurityException ex) {

			return false;
		}
	}

	public static void clearCache() {
		AnnotationTypeMappings.clearCache();
		AnnotationsScanner.clearCache();
		AttributeMethods.cache.clear();
		RepeatableContainers.cache.clear();
		OrderUtils.orderCache.clear();
	}

	private static class DefaultValueHolder {

		final Object defaultValue;

		public DefaultValueHolder(Object defaultValue) {
			this.defaultValue = defaultValue;
		}

		@Override
		public String toString() {
			return "*" + this.defaultValue;
		}
	}

}
```

## Apply Minification Excerpt

```java


package org.springframework.core.annotation;

import java.lang.annotation.Annotation;
import java.lang.reflect.AnnotatedElement;
import java.lang.reflect.Array;
import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.lang.reflect.Proxy;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.NoSuchElementException;
import java.util.Set;

import org.jspecify.annotations.Nullable;

import org.springframework.core.BridgeMethodResolver;
import org.springframework.core.annotation.AnnotationTypeMapping.MirrorSets.MirrorSet;
import org.springframework.core.annotation.MergedAnnotation.Adapt;
import org.springframework.core.annotation.MergedAnnotations.SearchStrategy;
import org.springframework.util.ClassUtils;
import org.springframework.util.CollectionUtils;
import org.springframework.util.ConcurrentReferenceHashMap;
import org.springframework.util.ReflectionUtils;
import org.springframework.util.StringUtils;


public abstract class AnnotationUtils {


	public static final String VALUE = MergedAnnotation.VALUE;

	private static final AnnotationFilter JAVA_LANG_ANNOTATION_FILTER =


... [truncated 20531 chars] ...

izedMergedAnnotationInvocationHandler);
		}
		catch (SecurityException ex) {


			return false;
		}
	}


	public static void clearCache() {
		AnnotationTypeMappings.clearCache();
		AnnotationsScanner.clearCache();
		AttributeMethods.cache.clear();
		RepeatableContainers.cache.clear();
		OrderUtils.orderCache.clear();
	}


	private static class DefaultValueHolder {

		final Object defaultValue;

		public DefaultValueHolder(Object defaultValue) {
			this.defaultValue = defaultValue;
		}

		@Override
		public String toString() {
			return "*" + this.defaultValue;
		}
	}

}
```

## Sync Minify Excerpt

```java


package org.springframework.core.annotation;

import java.lang.annotation.Annotation;
import java.lang.reflect.AnnotatedElement;
import java.lang.reflect.Array;
import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.lang.reflect.Proxy;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.NoSuchElementException;
import java.util.Set;

import org.jspecify.annotations.Nullable;

import org.springframework.core.BridgeMethodResolver;
import org.springframework.core.annotation.AnnotationTypeMapping.MirrorSets.MirrorSet;
import org.springframework.core.annotation.MergedAnnotation.Adapt;
import org.springframework.core.annotation.MergedAnnotations.SearchStrategy;
import org.springframework.util.ClassUtils;
import org.springframework.util.CollectionUtils;
import org.springframework.util.ConcurrentReferenceHashMap;
import org.springframework.util.ReflectionUtils;
import org.springframework.util.StringUtils;


public abstract class AnnotationUtils {


	public static final String VALUE = MergedAnnotation.VALUE;

	private static final AnnotationFilter JAVA_LANG_ANNOTATION_FILTER =


... [truncated 20531 chars] ...

izedMergedAnnotationInvocationHandler);
		}
		catch (SecurityException ex) {


			return false;
		}
	}


	public static void clearCache() {
		AnnotationTypeMappings.clearCache();
		AnnotationsScanner.clearCache();
		AttributeMethods.cache.clear();
		RepeatableContainers.cache.clear();
		OrderUtils.orderCache.clear();
	}


	private static class DefaultValueHolder {

		final Object defaultValue;

		public DefaultValueHolder(Object defaultValue) {
			this.defaultValue = defaultValue;
		}

		@Override
		public String toString() {
			return "*" + this.defaultValue;
		}
	}

}
```

## Async Minify Excerpt

```java


package org.springframework.core.annotation;

import java.lang.annotation.Annotation;
import java.lang.reflect.AnnotatedElement;
import java.lang.reflect.Array;
import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.lang.reflect.Proxy;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.NoSuchElementException;
import java.util.Set;

import org.jspecify.annotations.Nullable;

import org.springframework.core.BridgeMethodResolver;
import org.springframework.core.annotation.AnnotationTypeMapping.MirrorSets.MirrorSet;
import org.springframework.core.annotation.MergedAnnotation.Adapt;
import org.springframework.core.annotation.MergedAnnotations.SearchStrategy;
import org.springframework.util.ClassUtils;
import org.springframework.util.CollectionUtils;
import org.springframework.util.ConcurrentReferenceHashMap;
import org.springframework.util.ReflectionUtils;
import org.springframework.util.StringUtils;


public abstract class AnnotationUtils {


	public static final String VALUE = MergedAnnotation.VALUE;

	private static final AnnotationFilter JAVA_LANG_ANNOTATION_FILTER =


... [truncated 20531 chars] ...

izedMergedAnnotationInvocationHandler);
		}
		catch (SecurityException ex) {


			return false;
		}
	}


	public static void clearCache() {
		AnnotationTypeMappings.clearCache();
		AnnotationsScanner.clearCache();
		AttributeMethods.cache.clear();
		RepeatableContainers.cache.clear();
		OrderUtils.orderCache.clear();
	}


	private static class DefaultValueHolder {

		final Object defaultValue;

		public DefaultValueHolder(Object defaultValue) {
			this.defaultValue = defaultValue;
		}

		@Override
		public String toString() {
			return "*" + this.defaultValue;
		}
	}

}
```

## Symbols

```txt
  17| package org.springframework.core.annotation;
  19| import java.lang.annotation.Annotation;
  20| import java.lang.reflect.AnnotatedElement;
  21| import java.lang.reflect.Array;
  22| import java.lang.reflect.InvocationHandler;
  23| import java.lang.reflect.Method;
  24| import java.lang.reflect.Modifier;
  25| import java.lang.reflect.Proxy;
  26| import java.util.Collection;
  27| import java.util.Collections;
  28| import java.util.List;
  29| import java.util.Map;
  30| import java.util.NoSuchElementException;
  31| import java.util.Set;
  33| import org.jspecify.annotations.Nullable;
  35| import org.springframework.core.BridgeMethodResolver;
  36| import org.springframework.core.annotation.AnnotationTypeMapping.MirrorSets.MirrorSet;
  37| import org.springframework.core.annotation.MergedAnnotation.Adapt;
  38| import org.springframework.core.annotation.MergedAnnotations.SearchStrategy;
  39| import org.springframework.util.ClassUtils;
  40| import org.springframework.util.CollectionUtils;
  41| import org.springframework.util.ConcurrentReferenceHashMap;
  42| import org.springframework.util.ReflectionUtils;
  43| import org.springframework.util.StringUtils;
 110| public abstract class AnnotationUtils {
 115| 	public static final String VALUE = MergedAnnotation.VALUE;
 117| 	private static final AnnotationFilter JAVA_LANG_ANNOTATION_FILTER =
 120| 	private static final Map<Class<? extends Annotation>, Map<String, DefaultValueHolder>> defaultValuesCache =
 136| 	public static boolean isCandidateClass(Class<?> clazz, Collection<Class<? extends Annotation>> annotationTypes) {
 156| 	public static boolean isCandidateClass(Class<?> clazz, @Nullable Class<? extends Annotation> annotationType) {
 171| 	public static boolean isCandida

... [truncated 5457 chars] ...

1222| 	public static <A extends Annotation> A synthesizeAnnotation(
1223| 			A annotation, @Nullable AnnotatedElement annotatedElement) {
1246| 	public static <A extends Annotation> A synthesizeAnnotation(Class<A> annotationType) {
1279| 	public static <A extends Annotation> A synthesizeAnnotation(Map<String, Object> attributes,
1280| 			Class<A> annotationType, @Nullable AnnotatedElement annotatedElement) {
1307| 	static Annotation[] synthesizeAnnotationArray(Annotation[] annotations, AnnotatedElement annotatedElement) {
1327| 	public static boolean isSynthesizedAnnotation(@Nullable Annotation annotation) {
1343| 	public static void clearCache() {
1355| 	private static class DefaultValueHolder {
1357| 		final Object defaultValue;
1359| 		public DefaultValueHolder(Object defaultValue) {
1364| 		public String toString() {
```
