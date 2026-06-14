# Perl Module (.pm)

Source sample: `pm/perl-strict.pm`

Strategy: `aggressive`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 5491 | - | - |
| content-view | 5005 | 8.9% | 0.178 ms |
| applyMinification | 4384 | 20.2% | 0.194 ms |
| sync minify | 4384 | 20.2% | 0.186 ms |
| async minify | 4384 | 20.2% | 0.187 ms |
| symbols | 4228 | 23% | 0.037 ms |

## Notes

- aggressive text strategy.
- symbols are not implemented for this extension.

## Before Excerpt

```pm
package strict;

$strict::VERSION = "1.14";

my ( %bitmask, %explicit_bitmask );

BEGIN {
    # Verify that we're called correctly so that strictures will work.
    # Can't use Carp, since Carp uses us!
    # see also warnings.pm.
    die sprintf "Incorrect use of pragma '%s' at %s line %d.\n", __PACKAGE__, +(caller)[1,2]
        if __FILE__ !~ ( '(?x) \b     '.__PACKAGE__.'  \.pmc? \z' )
        && __FILE__ =~ ( '(?x) \b (?i:'.__PACKAGE__.') \.pmc? \z' );

    # which strictures are actually in force
    %bitmask = (
        refs => 0x00000002,
        subs => 0x00000200,
        vars => 0x00000400,
    );

    # which strictures have at some point been turned on or off explicitly
    # and must therefore not be touched by any subsequent `use VERSION` or `no VERSION`
    %explicit_bitmask = (
        refs => 0x00000020,
        subs => 0x00000040,
        vars => 0x00000080,
    );

    my $bits = 0;
    $bits |= $_ for values %bitmask;

    my $inline_all_bits = $bits;
    *all_bits = sub () { $inline_all_bits };

    $bits = 0;
    $bits |= $_ for values %explicit_bitmask;

    my $inline_all_explicit_bits = $bits;
    *all_explicit_bits = sub () { $inline_all_explicit_bits };
}

sub bits {
    my $do_

... [truncated 3691 chars] ...

ragma will abort with

    Unknown 'strict' tag(s) '...'

As of version 1.04 (Perl 5.10), strict verifies that it is used as
"strict" to avoid the dreaded Strict trap on case insensitive file
systems.

Beginning with Perl 5.12, use of "use VERSION" (where VERSION >= 5.11.0) now
lexically enables strictures just like "use strict" (in addition to the normal
"use VERSION" effects and features.)  In other words, "use v5.011" or higher
now implies "use strict" automatically, as noted in
L<perl5120delta/"Implicit strictures"> and L<C<use VERSION>|perlfunc/use VERSION>.

=cut

```

## Content-View Excerpt

```pm
package strict;

$strict::VERSION = "1.14";

my ( %bitmask, %explicit_bitmask );

BEGIN {

    die sprintf "Incorrect use of pragma '%s' at %s line %d.\n", __PACKAGE__, +(caller)[1,2]
        if __FILE__ !~ ( '(?x) \b     '.__PACKAGE__.'  \.pmc? \z' )
        && __FILE__ =~ ( '(?x) \b (?i:'.__PACKAGE__.') \.pmc? \z' );

    %bitmask = (
        refs => 0x00000002,
        subs => 0x00000200,
        vars => 0x00000400,
    );

    %explicit_bitmask = (
        refs => 0x00000020,
        subs => 0x00000040,
        vars => 0x00000080,
    );

    my $bits = 0;
    $bits |= $_ for values %bitmask;

    my $inline_all_bits = $bits;
    *all_bits = sub () { $inline_all_bits };

    $bits = 0;
    $bits |= $_ for values %explicit_bitmask;

    my $inline_all_explicit_bits = $bits;
    *all_explicit_bits = sub () { $inline_all_explicit_bits };
}

sub bits {
    my $do_explicit = caller eq __PACKAGE__;
    my $bits = 0;
    my @wrong;
    foreach my $s (@_) {
        if (exists $bitmask{$s}) {
            $bits |= $explicit_bitmask{$s} if $do_explicit;
            $bits |= $bitmask{$s};
        }
        else {
            push @wrong, $s;
        }
    }
    if (@wrong) {
        require Carp;
        Carp::cr

... [truncated 3205 chars] ...

pragma will abort with

    Unknown 'strict' tag(s) '...'

As of version 1.04 (Perl 5.10), strict verifies that it is used as
"strict" to avoid the dreaded Strict trap on case insensitive file
systems.

Beginning with Perl 5.12, use of "use VERSION" (where VERSION >= 5.11.0) now
lexically enables strictures just like "use strict" (in addition to the normal
"use VERSION" effects and features.)  In other words, "use v5.011" or higher
now implies "use strict" automatically, as noted in
L<perl5120delta/"Implicit strictures"> and L<C<use VERSION>|perlfunc/use VERSION>.

=cut
```

## Apply Minification Excerpt

```pm
package strict;$strict::VERSION = "1.14";my ( %bitmask,%explicit_bitmask );BEGIN{die sprintf "Incorrect use of pragma '%s' at %s line %d.\n",__PACKAGE__,+(caller)[1,2] if __FILE__ !~ ( '(?x) \b '.__PACKAGE__.' \.pmc? \z' ) && __FILE__ =~ ( '(?x) \b (?i:'.__PACKAGE__.') \.pmc? \z' );%bitmask = ( refs => 0x00000002,subs => 0x00000200,vars => 0x00000400,);%explicit_bitmask = ( refs => 0x00000020,subs => 0x00000040,vars => 0x00000080,);my $bits = 0;$bits |= $_ for values %bitmask;my $inline_all_bits = $bits;*all_bits = sub (){$inline_all_bits};$bits = 0;$bits |= $_ for values %explicit_bitmask;my $inline_all_explicit_bits = $bits;*all_explicit_bits = sub (){$inline_all_explicit_bits};}sub bits{my $do_explicit = caller eq __PACKAGE__;my $bits = 0;my @wrong;foreach my $s (@_){if (exists $bitmask{$s}){$bits |= $explicit_bitmask{$s}if $do_explicit;$bits |= $bitmask{$s};}else{push @wrong,$s;}}if (@wrong){require Carp;Carp::croak("Unknown 'strict' tag(s) '@wrong'");}$bits;}sub import{shift;$^H |= @_ ? &bits:all_bits | all_explicit_bits;}sub unimport{shift;if (@_){my $bits = &bits;$^H &= ~$bits;$^H |= all_explicit_bits & $bits;}else{$^H &= ~all_bits;$^H |= all_explicit_bits;}}1;__END__ =head1 NAME strict - Perl prag

... [truncated 2584 chars] ...

ed,the strict pragma will abort with Unknown 'strict' tag(s) '...' As of version 1.04 (Perl 5.10),strict verifies that it is used as "strict" to avoid the dreaded Strict trap on case insensitive file systems. Beginning with Perl 5.12,use of "use VERSION" (where VERSION>= 5.11.0) now lexically enables strictures just like "use strict" (in addition to the normal "use VERSION" effects and features.) In other words,"use v5.011" or higher now implies "use strict" automatically,as noted in L<perl5120delta/"Implicit strictures"> and L<C<use VERSION>|perlfunc/use VERSION>. =cut
```

## Sync Minify Excerpt

```pm
package strict;$strict::VERSION = "1.14";my ( %bitmask,%explicit_bitmask );BEGIN{die sprintf "Incorrect use of pragma '%s' at %s line %d.\n",__PACKAGE__,+(caller)[1,2] if __FILE__ !~ ( '(?x) \b '.__PACKAGE__.' \.pmc? \z' ) && __FILE__ =~ ( '(?x) \b (?i:'.__PACKAGE__.') \.pmc? \z' );%bitmask = ( refs => 0x00000002,subs => 0x00000200,vars => 0x00000400,);%explicit_bitmask = ( refs => 0x00000020,subs => 0x00000040,vars => 0x00000080,);my $bits = 0;$bits |= $_ for values %bitmask;my $inline_all_bits = $bits;*all_bits = sub (){$inline_all_bits};$bits = 0;$bits |= $_ for values %explicit_bitmask;my $inline_all_explicit_bits = $bits;*all_explicit_bits = sub (){$inline_all_explicit_bits};}sub bits{my $do_explicit = caller eq __PACKAGE__;my $bits = 0;my @wrong;foreach my $s (@_){if (exists $bitmask{$s}){$bits |= $explicit_bitmask{$s}if $do_explicit;$bits |= $bitmask{$s};}else{push @wrong,$s;}}if (@wrong){require Carp;Carp::croak("Unknown 'strict' tag(s) '@wrong'");}$bits;}sub import{shift;$^H |= @_ ? &bits:all_bits | all_explicit_bits;}sub unimport{shift;if (@_){my $bits = &bits;$^H &= ~$bits;$^H |= all_explicit_bits & $bits;}else{$^H &= ~all_bits;$^H |= all_explicit_bits;}}1;__END__ =head1 NAME strict - Perl prag

... [truncated 2584 chars] ...

ed,the strict pragma will abort with Unknown 'strict' tag(s) '...' As of version 1.04 (Perl 5.10),strict verifies that it is used as "strict" to avoid the dreaded Strict trap on case insensitive file systems. Beginning with Perl 5.12,use of "use VERSION" (where VERSION>= 5.11.0) now lexically enables strictures just like "use strict" (in addition to the normal "use VERSION" effects and features.) In other words,"use v5.011" or higher now implies "use strict" automatically,as noted in L<perl5120delta/"Implicit strictures"> and L<C<use VERSION>|perlfunc/use VERSION>. =cut
```

## Async Minify Excerpt

```pm
package strict;$strict::VERSION = "1.14";my ( %bitmask,%explicit_bitmask );BEGIN{die sprintf "Incorrect use of pragma '%s' at %s line %d.\n",__PACKAGE__,+(caller)[1,2] if __FILE__ !~ ( '(?x) \b '.__PACKAGE__.' \.pmc? \z' ) && __FILE__ =~ ( '(?x) \b (?i:'.__PACKAGE__.') \.pmc? \z' );%bitmask = ( refs => 0x00000002,subs => 0x00000200,vars => 0x00000400,);%explicit_bitmask = ( refs => 0x00000020,subs => 0x00000040,vars => 0x00000080,);my $bits = 0;$bits |= $_ for values %bitmask;my $inline_all_bits = $bits;*all_bits = sub (){$inline_all_bits};$bits = 0;$bits |= $_ for values %explicit_bitmask;my $inline_all_explicit_bits = $bits;*all_explicit_bits = sub (){$inline_all_explicit_bits};}sub bits{my $do_explicit = caller eq __PACKAGE__;my $bits = 0;my @wrong;foreach my $s (@_){if (exists $bitmask{$s}){$bits |= $explicit_bitmask{$s}if $do_explicit;$bits |= $bitmask{$s};}else{push @wrong,$s;}}if (@wrong){require Carp;Carp::croak("Unknown 'strict' tag(s) '@wrong'");}$bits;}sub import{shift;$^H |= @_ ? &bits:all_bits | all_explicit_bits;}sub unimport{shift;if (@_){my $bits = &bits;$^H &= ~$bits;$^H |= all_explicit_bits & $bits;}else{$^H &= ~all_bits;$^H |= all_explicit_bits;}}1;__END__ =head1 NAME strict - Perl prag

... [truncated 2584 chars] ...

ed,the strict pragma will abort with Unknown 'strict' tag(s) '...' As of version 1.04 (Perl 5.10),strict verifies that it is used as "strict" to avoid the dreaded Strict trap on case insensitive file systems. Beginning with Perl 5.12,use of "use VERSION" (where VERSION>= 5.11.0) now lexically enables strictures just like "use strict" (in addition to the normal "use VERSION" effects and features.) In other words,"use v5.011" or higher now implies "use strict" automatically,as noted in L<perl5120delta/"Implicit strictures"> and L<C<use VERSION>|perlfunc/use VERSION>. =cut
```

## Symbols

```txt
  1| package strict;
  3| $strict::VERSION = "1.14";
  5| my ( %bitmask, %explicit_bitmask );
  7| BEGIN {
 34|     *all_bits = sub () { $inline_all_bits };
 40|     *all_explicit_bits = sub () { $inline_all_explicit_bits };
 41| }
 43| sub bits {
 61| }
 63| sub import {
 66| }
 68| sub unimport {
 80| }
 82| 1;
 84| __END__
 86| =head1 NAME
 88| strict - Perl pragma to restrict unsafe constructs
 90| =head1 SYNOPSIS
 92|     use strict;
 94|     use strict "vars";
 95|     use strict "refs";
 96|     use strict "subs";
 98|     use strict;
 99|     no strict "vars";
101| =head1 DESCRIPTION
103| The C<strict> pragma disables certain Perl expressions that could behave
104| unexpectedly or are difficult to debug, turning them into errors. The
105| effect of this pragma is limited to the current file or scope block.
107| If no import list is supplied, all possible restrictions are assumed.
108| (This is the safest mode to operate in, but is sometimes too strict for
109| casual programming.)  Currently, there are three possible things to be
110| strict about:  "subs", "vars", and "refs".
112| =over 6
114| =item C<strict refs>
116| This generates a runtime error if you
117| use symbolic references (see L<perlref>).
119|     use strict 'refs';
120|     $ref = \$foo;
121|     print $$ref;	# ok
122|     $ref = "foo";
123|     print $$ref;	# runtime error; normally ok
124|     $file = "STDOUT";
125|     print $file "Hi!";	# error; note: no comma after $file
127| There is one exception to this rule:
129|     $bar = \&{'foo'};
130|     &$bar;
132| is allowed so that C<goto &$AUTOLOAD> would not break under stricture.
135| =item C<strict vars>
137| This generates a compile-time error if you access a variable that was
138| neither explicitly declared

... [truncated 1628 chars] ...

re C<< => >> or
180| inside curlies), but without forcing it always to a literal string.
182| Starting with Perl 5.8.1 strict is strict about its restrictions:
183| if unknown restrictions are used, the strict pragma will abort with
185|     Unknown 'strict' tag(s) '...'
187| As of version 1.04 (Perl 5.10), strict verifies that it is used as
188| "strict" to avoid the dreaded Strict trap on case insensitive file
189| systems.
191| Beginning with Perl 5.12, use of "use VERSION" (where VERSION >= 5.11.0) now
192| lexically enables strictures just like "use strict" (in addition to the normal
193| "use VERSION" effects and features.)  In other words, "use v5.011" or higher
194| now implies "use strict" automatically, as noted in
195| L<perl5120delta/"Implicit strictures"> and L<C<use VERSION>|perlfunc/use VERSION>.
197| =cut
```
