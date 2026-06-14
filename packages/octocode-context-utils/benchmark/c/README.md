# C (.c)

Source sample: `c/00-git-add.c`

Strategy: `conservative`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 18107 | - | - |
| content-view | 17413 | 3.8% | 0.662 ms |
| applyMinification | 17423 | 3.8% | 0.601 ms |
| sync minify | 17423 | 3.8% | 0.653 ms |
| async minify | 17423 | 3.8% | 0.652 ms |
| symbols | 5700 | 68.5% | 3.65 ms |

## Notes

- conservative text strategy.

## Before Excerpt

```c
/*
 * "git add" builtin command
 *
 * Copyright (C) 2006 Linus Torvalds
 */

#include "builtin.h"
#include "advice.h"
#include "config.h"
#include "environment.h"
#include "lockfile.h"
#include "editor.h"
#include "dir.h"
#include "gettext.h"
#include "pathspec.h"
#include "run-command.h"
#include "object-file.h"
#include "odb.h"
#include "odb/transaction.h"
#include "parse-options.h"
#include "path.h"
#include "preload-index.h"
#include "diff.h"
#include "read-cache.h"
#include "revision.h"
#include "strvec.h"
#include "submodule.h"
#include "add-interactive.h"

static const char * const builtin_add_usage[] = {
	N_("git add [<options>] [--] <pathspec>..."),
	NULL
};
static int patch_interactive, add_interactive, edit_interactive;
static struct interactive_options interactive_opts = INTERACTIVE_OPTIONS_INIT;
static int take_worktree_changes;
static int add_renormalize;
static int pathspec_file_nul;
static int include_sparse;
static const char *pathspec_from_file;

static int chmod_pathspec(struct repository *repo,
			  struct pathspec *pathspec,
			  char flip,
			  int show_only)
{
	int ret = 0;

	for (size_t i = 0; i < repo->index->cache_nr; i++) {
		struct cache_entry *ce = repo->index->cache[i];
		int

... [truncated 16307 chars] ...

(take_worktree_changes && !add_renormalize && !ignore_add_errors &&
	    report_path_error(ps_matched, &pathspec))
		exit(128);

	if (add_new_files)
		exit_status |= add_files(repo, &dir, flags);

	if (chmod_arg && pathspec.nr)
		exit_status |= chmod_pathspec(repo, &pathspec, chmod_arg[0], show_only);
	odb_transaction_commit(transaction);

finish:
	if (write_locked_index(repo->index, &lock_file,
			       COMMIT_LOCK | SKIP_IF_UNCHANGED))
		die(_("unable to write new index file"));

	free(ps_matched);
	dir_clear(&dir);
	clear_pathspec(&pathspec);
	return exit_status;
}

```

## Content-View Excerpt

```c
#include "builtin.h"
#include "advice.h"
#include "config.h"
#include "environment.h"
#include "lockfile.h"
#include "editor.h"
#include "dir.h"
#include "gettext.h"
#include "pathspec.h"
#include "run-command.h"
#include "object-file.h"
#include "odb.h"
#include "odb/transaction.h"
#include "parse-options.h"
#include "path.h"
#include "preload-index.h"
#include "diff.h"
#include "read-cache.h"
#include "revision.h"
#include "strvec.h"
#include "submodule.h"
#include "add-interactive.h"

static const char * const builtin_add_usage[] = {
	N_("git add [<options>] [--] <pathspec>..."),
	NULL
};
static int patch_interactive, add_interactive, edit_interactive;
static struct interactive_options interactive_opts = INTERACTIVE_OPTIONS_INIT;
static int take_worktree_changes;
static int add_renormalize;
static int pathspec_file_nul;
static int include_sparse;
static const char *pathspec_from_file;

static int chmod_pathspec(struct repository *repo,
			  struct pathspec *pathspec,
			  char flip,
			  int show_only)
{
	int ret = 0;

	for (size_t i = 0; i < repo->index->cache_nr; i++) {
		struct cache_entry *ce = repo->index->cache[i];
		int err;

		if (!include_sparse &&
		    (ce_skip_worktree(ce) ||
		     !path_i

... [truncated 15613 chars] ...

 (take_worktree_changes && !add_renormalize && !ignore_add_errors &&
	    report_path_error(ps_matched, &pathspec))
		exit(128);

	if (add_new_files)
		exit_status |= add_files(repo, &dir, flags);

	if (chmod_arg && pathspec.nr)
		exit_status |= chmod_pathspec(repo, &pathspec, chmod_arg[0], show_only);
	odb_transaction_commit(transaction);

finish:
	if (write_locked_index(repo->index, &lock_file,
			       COMMIT_LOCK | SKIP_IF_UNCHANGED))
		die(_("unable to write new index file"));

	free(ps_matched);
	dir_clear(&dir);
	clear_pathspec(&pathspec);
	return exit_status;
}
```

## Apply Minification Excerpt

```c


#include "builtin.h"
#include "advice.h"
#include "config.h"
#include "environment.h"
#include "lockfile.h"
#include "editor.h"
#include "dir.h"
#include "gettext.h"
#include "pathspec.h"
#include "run-command.h"
#include "object-file.h"
#include "odb.h"
#include "odb/transaction.h"
#include "parse-options.h"
#include "path.h"
#include "preload-index.h"
#include "diff.h"
#include "read-cache.h"
#include "revision.h"
#include "strvec.h"
#include "submodule.h"
#include "add-interactive.h"

static const char * const builtin_add_usage[] = {
	N_("git add [<options>] [--] <pathspec>..."),
	NULL
};
static int patch_interactive, add_interactive, edit_interactive;
static struct interactive_options interactive_opts = INTERACTIVE_OPTIONS_INIT;
static int take_worktree_changes;
static int add_renormalize;
static int pathspec_file_nul;
static int include_sparse;
static const char *pathspec_from_file;

static int chmod_pathspec(struct repository *repo,
			  struct pathspec *pathspec,
			  char flip,
			  int show_only)
{
	int ret = 0;

	for (size_t i = 0; i < repo->index->cache_nr; i++) {
		struct cache_entry *ce = repo->index->cache[i];
		int err;

		if (!include_sparse &&
		    (ce_skip_worktree(ce) ||
		     !path

... [truncated 15623 chars] ...

 (take_worktree_changes && !add_renormalize && !ignore_add_errors &&
	    report_path_error(ps_matched, &pathspec))
		exit(128);

	if (add_new_files)
		exit_status |= add_files(repo, &dir, flags);

	if (chmod_arg && pathspec.nr)
		exit_status |= chmod_pathspec(repo, &pathspec, chmod_arg[0], show_only);
	odb_transaction_commit(transaction);

finish:
	if (write_locked_index(repo->index, &lock_file,
			       COMMIT_LOCK | SKIP_IF_UNCHANGED))
		die(_("unable to write new index file"));

	free(ps_matched);
	dir_clear(&dir);
	clear_pathspec(&pathspec);
	return exit_status;
}
```

## Sync Minify Excerpt

```c


#include "builtin.h"
#include "advice.h"
#include "config.h"
#include "environment.h"
#include "lockfile.h"
#include "editor.h"
#include "dir.h"
#include "gettext.h"
#include "pathspec.h"
#include "run-command.h"
#include "object-file.h"
#include "odb.h"
#include "odb/transaction.h"
#include "parse-options.h"
#include "path.h"
#include "preload-index.h"
#include "diff.h"
#include "read-cache.h"
#include "revision.h"
#include "strvec.h"
#include "submodule.h"
#include "add-interactive.h"

static const char * const builtin_add_usage[] = {
	N_("git add [<options>] [--] <pathspec>..."),
	NULL
};
static int patch_interactive, add_interactive, edit_interactive;
static struct interactive_options interactive_opts = INTERACTIVE_OPTIONS_INIT;
static int take_worktree_changes;
static int add_renormalize;
static int pathspec_file_nul;
static int include_sparse;
static const char *pathspec_from_file;

static int chmod_pathspec(struct repository *repo,
			  struct pathspec *pathspec,
			  char flip,
			  int show_only)
{
	int ret = 0;

	for (size_t i = 0; i < repo->index->cache_nr; i++) {
		struct cache_entry *ce = repo->index->cache[i];
		int err;

		if (!include_sparse &&
		    (ce_skip_worktree(ce) ||
		     !path

... [truncated 15623 chars] ...

 (take_worktree_changes && !add_renormalize && !ignore_add_errors &&
	    report_path_error(ps_matched, &pathspec))
		exit(128);

	if (add_new_files)
		exit_status |= add_files(repo, &dir, flags);

	if (chmod_arg && pathspec.nr)
		exit_status |= chmod_pathspec(repo, &pathspec, chmod_arg[0], show_only);
	odb_transaction_commit(transaction);

finish:
	if (write_locked_index(repo->index, &lock_file,
			       COMMIT_LOCK | SKIP_IF_UNCHANGED))
		die(_("unable to write new index file"));

	free(ps_matched);
	dir_clear(&dir);
	clear_pathspec(&pathspec);
	return exit_status;
}
```

## Async Minify Excerpt

```c


#include "builtin.h"
#include "advice.h"
#include "config.h"
#include "environment.h"
#include "lockfile.h"
#include "editor.h"
#include "dir.h"
#include "gettext.h"
#include "pathspec.h"
#include "run-command.h"
#include "object-file.h"
#include "odb.h"
#include "odb/transaction.h"
#include "parse-options.h"
#include "path.h"
#include "preload-index.h"
#include "diff.h"
#include "read-cache.h"
#include "revision.h"
#include "strvec.h"
#include "submodule.h"
#include "add-interactive.h"

static const char * const builtin_add_usage[] = {
	N_("git add [<options>] [--] <pathspec>..."),
	NULL
};
static int patch_interactive, add_interactive, edit_interactive;
static struct interactive_options interactive_opts = INTERACTIVE_OPTIONS_INIT;
static int take_worktree_changes;
static int add_renormalize;
static int pathspec_file_nul;
static int include_sparse;
static const char *pathspec_from_file;

static int chmod_pathspec(struct repository *repo,
			  struct pathspec *pathspec,
			  char flip,
			  int show_only)
{
	int ret = 0;

	for (size_t i = 0; i < repo->index->cache_nr; i++) {
		struct cache_entry *ce = repo->index->cache[i];
		int err;

		if (!include_sparse &&
		    (ce_skip_worktree(ce) ||
		     !path

... [truncated 15623 chars] ...

 (take_worktree_changes && !add_renormalize && !ignore_add_errors &&
	    report_path_error(ps_matched, &pathspec))
		exit(128);

	if (add_new_files)
		exit_status |= add_files(repo, &dir, flags);

	if (chmod_arg && pathspec.nr)
		exit_status |= chmod_pathspec(repo, &pathspec, chmod_arg[0], show_only);
	odb_transaction_commit(transaction);

finish:
	if (write_locked_index(repo->index, &lock_file,
			       COMMIT_LOCK | SKIP_IF_UNCHANGED))
		die(_("unable to write new index file"));

	free(ps_matched);
	dir_clear(&dir);
	clear_pathspec(&pathspec);
	return exit_status;
}
```

## Symbols

```txt
  7| #include "builtin.h"
  8| #include "advice.h"
  9| #include "config.h"
 10| #include "environment.h"
 11| #include "lockfile.h"
 12| #include "editor.h"
 13| #include "dir.h"
 14| #include "gettext.h"
 15| #include "pathspec.h"
 16| #include "run-command.h"
 17| #include "object-file.h"
 18| #include "odb.h"
 19| #include "odb/transaction.h"
 20| #include "parse-options.h"
 21| #include "path.h"
 22| #include "preload-index.h"
 23| #include "diff.h"
 24| #include "read-cache.h"
 25| #include "revision.h"
 26| #include "strvec.h"
 27| #include "submodule.h"
 28| #include "add-interactive.h"
 30| static const char * const builtin_add_usage[] = {
 31| 	N_("git add [<options>] [--] <pathspec>..."),
 32| 	NULL
 33| };
 34| static int patch_interactive, add_interactive, edit_interactive;
 35| static struct interactive_options interactive_opts = INTERACTIVE_OPTIONS_INIT;
 36| static int take_worktree_changes;
 37| static int add_renormalize;
 38| static int pathspec_file_nul;
 39| static int include_sparse;
 40| static const char *pathspec_from_file;
 42| static int chmod_pathspec(struct repository *repo,
 43| 			  struct pathspec *pathspec,
 44| 			  char flip,
 45| 			  int show_only)
 46| {
 73| static int renormalize_tracked_files(struct repository *repo,
 74| 				     const struct pathspec *pathspec,
 75| 				     int flags)
 76| {
 99| static char *prune_directory(struct repository *repo,
100| 			     struct dir_struct *dir,
101| 			     struct pathspec *pathspec,
102| 			     int prefix)
103| {
123| static int refresh(struct repository *repo, int verbose, const struct pathspec *pathspec)
124| {
161| int interactive_add(struct repository *repo,
162| 		    const char **argv,
163| 		    const char *prefix,
164| 		    int patch, struct i

... [truncated 3100 chars] ...

bedded_advice[] = N_(
303| "You've added another git repository inside your current repository.\n"
304| "Clones of the outer repository will not contain the contents of\n"
305| "the embedded repository and will not know how to obtain it.\n"
306| "If you meant to add a submodule, use:\n"
307| "\n"
308| "	git submodule add <url> %s\n"
309| "\n"
310| "If you added this path by mistake, you can remove it from the\n"
311| "index with:\n"
312| "\n"
313| "	git rm --cached %s\n"
314| "\n"
315| "See \"git help submodule\" for more information."
316| );
318| static void check_embedded_repo(const char *path)
319| {
342| static int add_files(struct repository *repo, struct dir_struct *dir, int flags)
343| {
382| int cmd_add(int argc,
383| 	    const char **argv,
384| 	    const char *prefix,
385| 	    struct repository *repo)
386| {
```
