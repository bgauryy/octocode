# Perl (.pl)

Source sample: `pl/perl-checkcfgvar.pl`

Strategy: `aggressive`

Agent rating: **8.5/10 (strong)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 4523 | - | - | - |
| content-view | 3761 | 16.8% | 0.313 ms | 8.5/10 |
| applyMinification | 3101 | 31.4% | 0.331 ms | 8.5/10 |
| sync minify | 3101 | 31.4% | 0.333 ms | 8.5/10 |
| async minify | 3101 | 31.4% | 0.643 ms | 8.5/10 |
| symbols | n/a | n/a | 0.005 ms | n/a |

## Notes

- aggressive text strategy.
- symbols are not implemented for this extension.

## Before Excerpt

```pl
#!/usr/bin/perl

# Check that the various config.sh-clones have (at least) all the
# same symbols as the top-level config_h.SH so that the (potentially)
# needed symbols are not lagging after how Configure thinks the world
# is laid out.
#
# VMS is probably not handled properly here, due to their own
# rather elaborate DCL scripting.

use strict;
use warnings;
use autodie;

sub usage {
    my $err = shift and select STDERR;
    print "usage: $0 [--list] [--regen] [--default=value]\n";
    exit $err;
    } # usage

use Getopt::Long qw(:config bundling);
GetOptions (
    "help|?"      => sub { usage (0); },
    "l|list!"     => \(my $opt_l = 0),
    "regen"       => \(my $opt_r = 0),
    "default=s"   => \ my $default,
    "tap"         => \(my $tap   = 0),
    "v|verbose:1" => \(my $opt_v = 0),
    ) or usage (1);

$default and $default =~ s/^'(.*)'$/$1/; # Will be quoted on generation
my $test;

require './regen/regen_lib.pl' if $opt_r;

my $MASTER_CFG = "config_h.SH";
# Inclusive bounds on the main part of the file, $section == 1 below:
my $first = qr/^Author=/;
my $last = qr/^zip=/;

my @CFG = (
	   # we check from MANIFEST whether they are expected to be present.
	   # We can't base our check on $], be

... [truncated 2723 chars] ...

print the name once, however many problems
	    print "$cfg\n";
	} elsif ($opt_r && $cfg ne 'configure.com') {
	    if (defined $default) {
		push @{$lines[1]}, map {"$_='$default'\n"} @$missing;
	    } else {
		print "$cfg: missing '$_', use --default to add it\n"
		    foreach @$missing;
	    }

	    @{$lines[1]} = sort @{$lines[1]};
	    my $fh = open_new($cfg);
	    print $fh @{$_} foreach @lines;
	    close_and_rename($fh);
	} else {
	    print "$cfg: missing '$_'\n" foreach @$missing;
	}
    } elsif ($tap) {
	print "ok $test - $cfg has no missing keys\n";
    }
}

```

## Content-View Excerpt

```pl
#!/usr/bin/perl

use strict;
use warnings;
use autodie;

sub usage {
    my $err = shift and select STDERR;
    print "usage: $0 [--list] [--regen] [--default=value]\n";
    exit $err;
    }

use Getopt::Long qw(:config bundling);
GetOptions (
    "help|?"      => sub { usage (0); },
    "l|list!"     => \(my $opt_l = 0),
    "regen"       => \(my $opt_r = 0),
    "default=s"   => \ my $default,
    "tap"         => \(my $tap   = 0),
    "v|verbose:1" => \(my $opt_v = 0),
    ) or usage (1);

$default and $default =~ s/^'(.*)'$/$1/;
my $test;

require './regen/regen_lib.pl' if $opt_r;

my $MASTER_CFG = "config_h.SH";

my $first = qr/^Author=/;
my $last = qr/^zip=/;

my @CFG = (

	   "Cross/config.sh-arm-linux",
	   "Cross/config.sh-arm-linux-n770",
	   "plan9/config_sh.sample",
	   "win32/config.gc",
	   "win32/config.vc",
	   "configure.com",
	   "Porting/config.sh",
	  );

my @MASTER_CFG;
{
    my %seen;
    $opt_v and warn "Reading $MASTER_CFG ...\n";
    open my $fh, '<', $MASTER_CFG;
    while (<$fh>) {
	while (/[^\\]\$([a-z]\w+)/g) {
	    my $v = $1;
	    next if $v =~ /^(CONFIG_H|CONFIG_SH)$/;
	    $seen{$v}++;
	}
    }
    close $fh;
    @MASTER_CFG = sort keys %seen;
}

my %MANIFEST;

{
    $opt_

... [truncated 1961 chars] ...

 print the name once, however many problems
	    print "$cfg\n";
	} elsif ($opt_r && $cfg ne 'configure.com') {
	    if (defined $default) {
		push @{$lines[1]}, map {"$_='$default'\n"} @$missing;
	    } else {
		print "$cfg: missing '$_', use --default to add it\n"
		    foreach @$missing;
	    }

	    @{$lines[1]} = sort @{$lines[1]};
	    my $fh = open_new($cfg);
	    print $fh @{$_} foreach @lines;
	    close_and_rename($fh);
	} else {
	    print "$cfg: missing '$_'\n" foreach @$missing;
	}
    } elsif ($tap) {
	print "ok $test - $cfg has no missing keys\n";
    }
}
```

## Apply Minification Excerpt

```pl
#!/usr/bin/perl use strict;use warnings;use autodie;sub usage{my $err = shift and select STDERR;print "usage:$0 [--list] [--regen] [--default=value]\n";exit $err;}use Getopt::Long qw(:config bundling);GetOptions ( "help|?" => sub{usage (0);},"l|list!" => \(my $opt_l = 0),"regen" => \(my $opt_r = 0),"default=s" => \ my $default,"tap" => \(my $tap = 0),"v|verbose:1" => \(my $opt_v = 0),) or usage (1);$default and $default =~ s/^'(.*)'$/$1/;my $test;require './regen/regen_lib.pl' if $opt_r;my $MASTER_CFG = "config_h.SH";my $first = qr/^Author=/;my $last = qr/^zip=/;my @CFG = ( "Cross/config.sh-arm-linux","Cross/config.sh-arm-linux-n770","plan9/config_sh.sample","win32/config.gc","win32/config.vc","configure.com","Porting/config.sh",);my @MASTER_CFG;{my %seen;$opt_v and warn "Reading $MASTER_CFG ...\n";open my $fh,'<',$MASTER_CFG;while (<$fh>){while (/[^\\]\$([a-z]\w+)/g){my $v = $1;next if $v =~ /^(CONFIG_H|CONFIG_SH)$/;$seen{$v}++;}}close $fh;@MASTER_CFG = sort keys %seen;}my %MANIFEST;{$opt_v and warn "Reading MANIFEST ...\n";open my $fh,'<','MANIFEST';while (<$fh>){$MANIFEST{$1}++ if /^(.+?)\t/;}close $fh;}printf "1..%d\n",2 * @CFG if $tap;for my $cfg (sort @CFG){unless (exists $MANIFEST{$cfg}){warn "[ski

... [truncated 1301 chars] ...

}++$test;if ($missing){if ($tap){print "not ok $test - $cfg missing keys @$missing\n";}elsif ($opt_l){# print the name once,however many problems print "$cfg\n";}elsif ($opt_r && $cfg ne 'configure.com'){if (defined $default){push @{$lines[1]},map{"$_='$default'\n"}@$missing;}else{print "$cfg:missing '$_',use --default to add it\n" foreach @$missing;}@{$lines[1]}= sort @{$lines[1]};my $fh = open_new($cfg);print $fh @{$_}foreach @lines;close_and_rename($fh);}else{print "$cfg:missing '$_'\n" foreach @$missing;}}elsif ($tap){print "ok $test - $cfg has no missing keys\n";}}
```

## Sync Minify Excerpt

```pl
#!/usr/bin/perl use strict;use warnings;use autodie;sub usage{my $err = shift and select STDERR;print "usage:$0 [--list] [--regen] [--default=value]\n";exit $err;}use Getopt::Long qw(:config bundling);GetOptions ( "help|?" => sub{usage (0);},"l|list!" => \(my $opt_l = 0),"regen" => \(my $opt_r = 0),"default=s" => \ my $default,"tap" => \(my $tap = 0),"v|verbose:1" => \(my $opt_v = 0),) or usage (1);$default and $default =~ s/^'(.*)'$/$1/;my $test;require './regen/regen_lib.pl' if $opt_r;my $MASTER_CFG = "config_h.SH";my $first = qr/^Author=/;my $last = qr/^zip=/;my @CFG = ( "Cross/config.sh-arm-linux","Cross/config.sh-arm-linux-n770","plan9/config_sh.sample","win32/config.gc","win32/config.vc","configure.com","Porting/config.sh",);my @MASTER_CFG;{my %seen;$opt_v and warn "Reading $MASTER_CFG ...\n";open my $fh,'<',$MASTER_CFG;while (<$fh>){while (/[^\\]\$([a-z]\w+)/g){my $v = $1;next if $v =~ /^(CONFIG_H|CONFIG_SH)$/;$seen{$v}++;}}close $fh;@MASTER_CFG = sort keys %seen;}my %MANIFEST;{$opt_v and warn "Reading MANIFEST ...\n";open my $fh,'<','MANIFEST';while (<$fh>){$MANIFEST{$1}++ if /^(.+?)\t/;}close $fh;}printf "1..%d\n",2 * @CFG if $tap;for my $cfg (sort @CFG){unless (exists $MANIFEST{$cfg}){warn "[ski

... [truncated 1301 chars] ...

}++$test;if ($missing){if ($tap){print "not ok $test - $cfg missing keys @$missing\n";}elsif ($opt_l){# print the name once,however many problems print "$cfg\n";}elsif ($opt_r && $cfg ne 'configure.com'){if (defined $default){push @{$lines[1]},map{"$_='$default'\n"}@$missing;}else{print "$cfg:missing '$_',use --default to add it\n" foreach @$missing;}@{$lines[1]}= sort @{$lines[1]};my $fh = open_new($cfg);print $fh @{$_}foreach @lines;close_and_rename($fh);}else{print "$cfg:missing '$_'\n" foreach @$missing;}}elsif ($tap){print "ok $test - $cfg has no missing keys\n";}}
```

## Async Minify Excerpt

```pl
#!/usr/bin/perl use strict;use warnings;use autodie;sub usage{my $err = shift and select STDERR;print "usage:$0 [--list] [--regen] [--default=value]\n";exit $err;}use Getopt::Long qw(:config bundling);GetOptions ( "help|?" => sub{usage (0);},"l|list!" => \(my $opt_l = 0),"regen" => \(my $opt_r = 0),"default=s" => \ my $default,"tap" => \(my $tap = 0),"v|verbose:1" => \(my $opt_v = 0),) or usage (1);$default and $default =~ s/^'(.*)'$/$1/;my $test;require './regen/regen_lib.pl' if $opt_r;my $MASTER_CFG = "config_h.SH";my $first = qr/^Author=/;my $last = qr/^zip=/;my @CFG = ( "Cross/config.sh-arm-linux","Cross/config.sh-arm-linux-n770","plan9/config_sh.sample","win32/config.gc","win32/config.vc","configure.com","Porting/config.sh",);my @MASTER_CFG;{my %seen;$opt_v and warn "Reading $MASTER_CFG ...\n";open my $fh,'<',$MASTER_CFG;while (<$fh>){while (/[^\\]\$([a-z]\w+)/g){my $v = $1;next if $v =~ /^(CONFIG_H|CONFIG_SH)$/;$seen{$v}++;}}close $fh;@MASTER_CFG = sort keys %seen;}my %MANIFEST;{$opt_v and warn "Reading MANIFEST ...\n";open my $fh,'<','MANIFEST';while (<$fh>){$MANIFEST{$1}++ if /^(.+?)\t/;}close $fh;}printf "1..%d\n",2 * @CFG if $tap;for my $cfg (sort @CFG){unless (exists $MANIFEST{$cfg}){warn "[ski

... [truncated 1301 chars] ...

}++$test;if ($missing){if ($tap){print "not ok $test - $cfg missing keys @$missing\n";}elsif ($opt_l){# print the name once,however many problems print "$cfg\n";}elsif ($opt_r && $cfg ne 'configure.com'){if (defined $default){push @{$lines[1]},map{"$_='$default'\n"}@$missing;}else{print "$cfg:missing '$_',use --default to add it\n" foreach @$missing;}@{$lines[1]}= sort @{$lines[1]};my $fh = open_new($cfg);print $fh @{$_}foreach @lines;close_and_rename($fh);}else{print "$cfg:missing '$_'\n" foreach @$missing;}}elsif ($tap){print "ok $test - $cfg has no missing keys\n";}}
```

## Symbols

```txt
No symbols returned for this sample.
```
