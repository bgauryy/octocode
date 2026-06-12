# Perl (.pl)

Source sample: `pl/perl-checkcfgvar.pl`

Strategy: `aggressive`

Agent rating: **8.5/10 (strong)**

Agent understanding from minified output: **8.2/10 (strong)**

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
| content-view | 3761 | 16.8% | 0.667 ms | 8.5/10 |
| applyMinification | 3101 | 31.4% | 0.719 ms | 8.5/10 |
| sync minify | 3101 | 31.4% | 0.686 ms | 8.5/10 |
| async minify | 3101 | 31.4% | 0.708 ms | 8.5/10 |
| symbols | 1215 | 73.1% | 0.204 ms | n/a |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 6.7/10 (2/3) |
| delimiter structure | 10/10 |
| output health | 10/10 |
| context budget | 8/10 |
| symbol context | 7/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 4523 | 0% | 10/10 excellent | 6.7/10 | 9.9/10 |
| standard | 3761 | 16.8% | 8.2/10 strong | 6.7/10 | 10/10 |
| minify | 3101 | 31.4% | 8.3/10 strong | 6.7/10 | 10/10 |
| symbols | 1215 | 73.1% | 8.6/10 strong | 6.7/10 | 9.8/10 |

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
  1| #!/usr/bin/perl
 11| use strict;
 12| use warnings;
 13| use autodie;
 15| sub usage {
 19|     } # usage
 21| use Getopt::Long qw(:config bundling);
 22| GetOptions (
 23|     "help|?"      => sub { usage (0); },
 24|     "l|list!"     => \(my $opt_l = 0),
 25|     "regen"       => \(my $opt_r = 0),
 26|     "default=s"   => \ my $default,
 27|     "tap"         => \(my $tap   = 0),
 28|     "v|verbose:1" => \(my $opt_v = 0),
 29|     ) or usage (1);
 31| $default and $default =~ s/^'(.*)'$/$1/; # Will be quoted on generation
 32| my $test;
 34| require './regen/regen_lib.pl' if $opt_r;
 36| my $MASTER_CFG = "config_h.SH";
 38| my $first = qr/^Author=/;
 39| my $last = qr/^zip=/;
 41| my @CFG = (
 45| 	   "Cross/config.sh-arm-linux",
 46| 	   "Cross/config.sh-arm-linux-n770",
 47| 	   "plan9/config_sh.sample",
 48| 	   "win32/config.gc",
 49| 	   "win32/config.vc",
 50| 	   "configure.com",
 51| 	   "Porting/config.sh",
 52| 	  );
 54| my @MASTER_CFG;
 55| {
 68| }
 70| my %MANIFEST;
 72| {
 79| }
 81| printf "1..%d\n", 2 * @CFG if $tap;
 83| for my $cfg (sort @CFG) {
 84|     unless (exists $MANIFEST{$cfg}) {
132|     } elsif (join("", @{$lines[1]}) eq join("", sort @{$lines[1]})) {
176| }
```
