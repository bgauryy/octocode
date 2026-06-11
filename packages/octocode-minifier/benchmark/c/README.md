# C (.c)

Source sample: `c/git-add.c`

Strategy: `conservative`

Agent rating: **7.6/10 (good)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 18107 | - | - | - |
| content-view | 17413 | 3.8% | 8.698 ms | 6.3/10 |
| applyMinification | 17413 | 3.8% | 4.541 ms | 6.3/10 |
| sync minify | 17413 | 3.8% | 3.122 ms | 6.3/10 |
| async minify | 17413 | 3.8% | 2.917 ms | 6.3/10 |
| symbols | 2036 | 88.8% | 0.789 ms | 10/10 |

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
 42| static int chmod_pathspec(struct repository *repo,
 43| 			  struct pathspec *pathspec,
 44| 			  char flip,
 45| 			  int show_only)
 73| static int renormalize_tracked_files(struct repository *repo,
 74| 				     const struct pathspec *pathspec,
 75| 				     int flags)
 99| static char *prune_directory(struct repository *repo,
100| 			     struct dir_struct *dir,
101| 			     struct pathspec *pathspec,
102| 			     int prefix)
123| static int refresh(struct repository *repo, int verbose, const struct pathspec *pathspec)
161| int interactive_add(struct repository *repo,
162| 		    const char **argv,
163| 		    const char *prefix,
164| 		    int patch, struct interactive_options *interactive_opts)
184| static int edit_patch(struct repository *repo,
185| 		      int argc,
186| 		      const char **argv,
187| 		      const char *prefix)
233| N_("The following paths are ignored by one of your .gitignore files:\n");
239| #define ADDREMOVE_DEFAULT 1
245| static int ignore_removal_cb(const struct option *opt, const char *arg, int unset)
287| static int add_config(const char *var, const char *value,
288| 		      const struct config_context *ctx, void *cb)
318| static void check_embedded_repo(const char *path)
342| static int add_files(struct repository *repo, struct dir_struct *dir, int flags)
382| int cmd_add(int argc,
383| 	    const char **argv,
384| 	    const char *prefix,
385| 	    struct repository *repo)
```
