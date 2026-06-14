# R (.r)

Source sample: `r/dplyr-mutate.R`

Strategy: `aggressive`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 15796 | - | - |
| content-view | 8436 | 46.6% | 0.347 ms |
| applyMinification | 6643 | 57.9% | 0.403 ms |
| sync minify | 6643 | 57.9% | 0.349 ms |
| async minify | 6643 | 57.9% | 0.327 ms |
| symbols | 1540 | 90.3% | 0.055 ms |

## Notes

- aggressive text strategy.
- symbols are not implemented for this extension.

## Before Excerpt

```r
#' Create, modify, and delete columns
#'
#' `mutate()` creates new columns that are functions of existing variables.
#' It can also modify (if the name is the same as an existing
#' column) and delete columns (by setting their value to `NULL`).
#'
#' @section Useful mutate functions:
#'
#' * [`+`], [`-`], [log()], etc., for their usual mathematical meanings
#'
#' * [lead()], [lag()]
#'
#' * [dense_rank()], [min_rank()], [percent_rank()], [row_number()],
#'   [cume_dist()], [ntile()]
#'
#' * [cumsum()], [cummean()], [cummin()], [cummax()], [cumany()], [cumall()]
#'
#' * [na_if()], [coalesce()]
#'
#' * [if_else()], [recode()], [case_when()]
#'
#' @section Grouped tibbles:
#'
#' Because mutating expressions are computed within groups, they may
#' yield different results on grouped tibbles. This will be the case
#' as soon as an aggregating, lagging, or ranking function is
#' involved. Compare this ungrouped mutate:
#'
#' ```
#' starwars |>
#'   select(name, mass, species) |>
#'   mutate(mass_norm = mass / mean(mass, na.rm = TRUE))
#' ```
#'
#' With the grouped equivalent:
#'
#' ```
#' starwars |>
#'   select(name, mass, species) |>
#'   group_by(species) |>
#'   mutate(mass_norm = mass / mean(mass, na.rm = T

... [truncated 13996 chars] ...

  data_size <- cnd$data_size
  c(
    glue(
      "Inlined constant `{label}` must be size {or_1(data_size)}, not {constant_size}."
    )
  )
}

check_muffled_warning <- function(cnd) {
  early_exit <- TRUE

  # Cancel early exits, e.g. from an exiting handler. This way we can
  # still instrument caught warnings to avoid confusing
  # inconsistencies.
  on.exit(
    if (early_exit) {
      return(FALSE)
    }
  )

  muffled <- withRestarts(
    muffleWarning = function(...) TRUE,
    {
      signalCondition(cnd)
      FALSE
    }
  )

  early_exit <- FALSE
  muffled
}

```

## Content-View Excerpt

```r
mutate <- function(.data, ...) {
  UseMethod("mutate")
}

mutate.data.frame <- function(
  .data,
  ...,
  .by = NULL,
  .keep = c("all", "used", "unused", "none"),
  .before = NULL,
  .after = NULL
) {
  keep <- arg_match0(.keep, values = c("all", "used", "unused", "none"))

  by <- compute_by({{ .by }}, .data, by_arg = ".by", data_arg = ".data")

  cols <- mutate_cols(.data, dplyr_quosures(...), by)
  used <- attr(cols, "used")

  out <- dplyr_col_modify(.data, cols)

  names_original <- names(.data)

  out <- mutate_relocate(
    out = out,
    before = {{ .before }},
    after = {{ .after }},
    names_original = names_original
  )

  names_new <- names(cols)
  names_groups <- by$names

  out <- mutate_keep(
    out = out,
    keep = keep,
    used = used,
    names_new = names_new,
    names_groups = names_groups
  )

  out
}

mutate_relocate <- function(out, before, after, names_original) {
  before <- enquo(before)
  after <- enquo(after)

  if (quo_is_null(before) && quo_is_null(after)) {
    return(out)
  }

  names <- names(out)
  names <- setdiff(names, names_original)

  relocate(
    out,
    all_of(names),
    .before = !!before,
    .after = !!after
  )
}

mutate_keep <- function(out, keep,

... [truncated 6636 chars] ...



`mutate_bullets.dplyr:::mutate_constant_recycle_error` <- function(cnd, ...) {
  label <- ctxt_error_label()
  constant_size <- cnd$constant_size
  data_size <- cnd$data_size
  c(
    glue(
      "Inlined constant `{label}` must be size {or_1(data_size)}, not {constant_size}."
    )
  )
}

check_muffled_warning <- function(cnd) {
  early_exit <- TRUE

  on.exit(
    if (early_exit) {
      return(FALSE)
    }
  )

  muffled <- withRestarts(
    muffleWarning = function(...) TRUE,
    {
      signalCondition(cnd)
      FALSE
    }
  )

  early_exit <- FALSE
  muffled
}
```

## Apply Minification Excerpt

```r
mutate<- function(.data,...){UseMethod("mutate")}mutate.data.frame<- function( .data,...,.by = NULL,.keep = c("all","used","unused","none"),.before = NULL,.after = NULL ){keep<- arg_match0(.keep,values = c("all","used","unused","none")) by<- compute_by({{.by}},.data,by_arg = ".by",data_arg = ".data") cols<- mutate_cols(.data,dplyr_quosures(...),by) used<- attr(cols,"used") out<- dplyr_col_modify(.data,cols) names_original<- names(.data) out<- mutate_relocate( out = out,before ={{.before}},after ={{.after}},names_original = names_original ) names_new<- names(cols) names_groups<- by$names out<- mutate_keep( out = out,keep = keep,used = used,names_new = names_new,names_groups = names_groups ) out}mutate_relocate<- function(out,before,after,names_original){before<- enquo(before) after<- enquo(after) if (quo_is_null(before) && quo_is_null(after)){return(out)}names<- names(out) names<- setdiff(names,names_original) relocate( out,all_of(names),.before = !!before,.after = !!after )}mutate_keep<- function(out,keep,used,names_new,names_groups){names<- names(out) if (keep == "all"){names_out<- names}else{names_keep<- switch( keep,used = names(used)[used],unused = names(used)[!used],none = character(),abort("Unknown

... [truncated 4843 chars] ...

 = cnd_bullet_rowwise_unlist() )}`mutate_bullets.dplyr:::error_incompatible_combine`<- function(cnd,...){c()}`mutate_bullets.dplyr:::mutate_constant_recycle_error`<- function(cnd,...){label<- ctxt_error_label() constant_size<- cnd$constant_size data_size<- cnd$data_size c( glue( "Inlined constant `{label}` must be size{or_1(data_size)},not{constant_size}." ) )}check_muffled_warning<- function(cnd){early_exit<- TRUE on.exit( if (early_exit){return(FALSE)}) muffled<- withRestarts( muffleWarning = function(...) TRUE,{signalCondition(cnd) FALSE}) early_exit<- FALSE muffled}
```

## Sync Minify Excerpt

```r
mutate<- function(.data,...){UseMethod("mutate")}mutate.data.frame<- function( .data,...,.by = NULL,.keep = c("all","used","unused","none"),.before = NULL,.after = NULL ){keep<- arg_match0(.keep,values = c("all","used","unused","none")) by<- compute_by({{.by}},.data,by_arg = ".by",data_arg = ".data") cols<- mutate_cols(.data,dplyr_quosures(...),by) used<- attr(cols,"used") out<- dplyr_col_modify(.data,cols) names_original<- names(.data) out<- mutate_relocate( out = out,before ={{.before}},after ={{.after}},names_original = names_original ) names_new<- names(cols) names_groups<- by$names out<- mutate_keep( out = out,keep = keep,used = used,names_new = names_new,names_groups = names_groups ) out}mutate_relocate<- function(out,before,after,names_original){before<- enquo(before) after<- enquo(after) if (quo_is_null(before) && quo_is_null(after)){return(out)}names<- names(out) names<- setdiff(names,names_original) relocate( out,all_of(names),.before = !!before,.after = !!after )}mutate_keep<- function(out,keep,used,names_new,names_groups){names<- names(out) if (keep == "all"){names_out<- names}else{names_keep<- switch( keep,used = names(used)[used],unused = names(used)[!used],none = character(),abort("Unknown

... [truncated 4843 chars] ...

 = cnd_bullet_rowwise_unlist() )}`mutate_bullets.dplyr:::error_incompatible_combine`<- function(cnd,...){c()}`mutate_bullets.dplyr:::mutate_constant_recycle_error`<- function(cnd,...){label<- ctxt_error_label() constant_size<- cnd$constant_size data_size<- cnd$data_size c( glue( "Inlined constant `{label}` must be size{or_1(data_size)},not{constant_size}." ) )}check_muffled_warning<- function(cnd){early_exit<- TRUE on.exit( if (early_exit){return(FALSE)}) muffled<- withRestarts( muffleWarning = function(...) TRUE,{signalCondition(cnd) FALSE}) early_exit<- FALSE muffled}
```

## Async Minify Excerpt

```r
mutate<- function(.data,...){UseMethod("mutate")}mutate.data.frame<- function( .data,...,.by = NULL,.keep = c("all","used","unused","none"),.before = NULL,.after = NULL ){keep<- arg_match0(.keep,values = c("all","used","unused","none")) by<- compute_by({{.by}},.data,by_arg = ".by",data_arg = ".data") cols<- mutate_cols(.data,dplyr_quosures(...),by) used<- attr(cols,"used") out<- dplyr_col_modify(.data,cols) names_original<- names(.data) out<- mutate_relocate( out = out,before ={{.before}},after ={{.after}},names_original = names_original ) names_new<- names(cols) names_groups<- by$names out<- mutate_keep( out = out,keep = keep,used = used,names_new = names_new,names_groups = names_groups ) out}mutate_relocate<- function(out,before,after,names_original){before<- enquo(before) after<- enquo(after) if (quo_is_null(before) && quo_is_null(after)){return(out)}names<- names(out) names<- setdiff(names,names_original) relocate( out,all_of(names),.before = !!before,.after = !!after )}mutate_keep<- function(out,keep,used,names_new,names_groups){names<- names(out) if (keep == "all"){names_out<- names}else{names_keep<- switch( keep,used = names(used)[used],unused = names(used)[!used],none = character(),abort("Unknown

... [truncated 4843 chars] ...

 = cnd_bullet_rowwise_unlist() )}`mutate_bullets.dplyr:::error_incompatible_combine`<- function(cnd,...){c()}`mutate_bullets.dplyr:::mutate_constant_recycle_error`<- function(cnd,...){label<- ctxt_error_label() constant_size<- cnd$constant_size data_size<- cnd$data_size c( glue( "Inlined constant `{label}` must be size{or_1(data_size)},not{constant_size}." ) )}check_muffled_warning<- function(cnd){early_exit<- TRUE on.exit( if (early_exit){return(FALSE)}) muffled<- withRestarts( muffleWarning = function(...) TRUE,{signalCondition(cnd) FALSE}) early_exit<- FALSE muffled}
```

## Symbols

```txt
145| mutate <- function(.data, ...) {
147| }
171| mutate.data.frame <- function(
172|   .data,
173|   ...,
174|   .by = NULL,
175|   .keep = c("all", "used", "unused", "none"),
176|   .before = NULL,
177|   .after = NULL
178| ) {
181|   by <- compute_by({{ .by }}, .data, by_arg = ".by", data_arg = ".data")
192|     before = {{ .before }},
193|     after = {{ .after }},
209| }
213| mutate_relocate <- function(out, before, after, names_original) {
232| }
234| mutate_keep <- function(out, keep, used, names_new, names_groups) {
251| }
253| mutate_cols <- function(data, dots, by, error_call = caller_env()) {
301| }
303| mutate_col <- function(dot, data, mask, new_columns) {
468| }
470| mutate_bullets <- function(cnd, ...) {
472| }
475| `mutate_bullets.dplyr:::mutate_incompatible_size` <- function(cnd, ...) {
481|     glue("`{label}` must be size {or_1(expected_size)}, not {result_size}."),
484| }
486| `mutate_bullets.dplyr:::mutate_mixed_null` <- function(cnd, ...) {
489|     glue("`{label}` must return compatible vectors across groups."),
493| }
495| `mutate_bullets.dplyr:::mutate_not_vector` <- function(cnd, ...) {
499|     glue("`{label}` must be a vector, not {obj_type_friendly(result)}."),
502| }
504| `mutate_bullets.dplyr:::error_incompatible_combine` <- function(cnd, ...) {
507| }
509| `mutate_bullets.dplyr:::mutate_constant_recycle_error` <- function(cnd, ...) {
515|       "Inlined constant `{label}` must be size {or_1(data_size)}, not {constant_size}."
518| }
520| check_muffled_warning <- function(cnd) {
542| }
```
