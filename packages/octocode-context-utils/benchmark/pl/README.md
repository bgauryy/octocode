# Perl (.pl)

Source sample: `pl/perl-checkcfgvar.pl`

Strategy: `aggressive`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 4523 | - | - |
| content-view | 3761 | 16.8% | 0.102 ms |
| applyMinification | 3101 | 31.4% | 0.108 ms |
| sync minify | 3101 | 31.4% | 0.121 ms |
| async minify | 3101 | 31.4% | 0.122 ms |
| symbols | 1215 | 73.1% | 0.028 ms |

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
