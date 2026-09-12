import builtins
seen = []
def deco(value):
    seen.append(value)
    return lambda obj: obj
@deco(tuple)
def f[tuple](arg=tuple, *, kw=tuple) -> tuple:
    def nested():
        return tuple
    return tuple, nested(), [tuple for _ in (0,)][0]
assert seen == [builtins.tuple]
assert f.__defaults__ == (builtins.tuple,)
assert f.__kwdefaults__ == {'kw': builtins.tuple}
assert f.__annotations__['return'] is f.__type_params__[0]
assert all(x is f.__type_params__[0] for x in f())
class Meta(type):
    def __new__(cls, name, bases, ns, **kw):
        seen.append(kw)
        return super().__new__(cls, name, bases, ns)
class Base:
    pass
def base(value):
    seen.append(value)
    return Base
@deco(tuple)
class BaseProbe[tuple](base(tuple)):
    pass
assert seen[-2] is builtins.tuple
assert seen[-1] is BaseProbe.__type_params__[0]

class Outer[tuple]:
    all = 'class all'
    class Inner[T](metaclass=Meta, value=all, tp=tuple,
                   comp=[all for _ in (0,)][0], lam=(lambda: all)()):
        ordinary = all
        generic = tuple
        def method[U](self) -> all:
            return all, tuple
assert seen[-1]['value'] == 'class all'
assert seen[-1]['tp'] is Outer.__type_params__[0]
assert seen[-1]['comp'] is builtins.all
assert seen[-1]['lam'] is builtins.all
assert Outer.Inner.ordinary is builtins.all
assert Outer.Inner.generic is Outer.__type_params__[0]
# method annotations see their immediately enclosing class, not Outer.
assert Outer.Inner.method.__annotations__['return'] is builtins.all
assert Outer.Inner().method() == (builtins.all, Outer.__type_params__[0])
class Ann:
    all = 'annotation class all'
    def method[T: all = all](self, arg: all) -> T:
        return all
assert Ann.method.__annotations__['arg'] == 'annotation class all'
assert Ann.method.__type_params__[0].__bound__ == 'annotation class all'
assert Ann.method.__type_params__[0].__default__ == 'annotation class all'
assert Ann().method(None) is builtins.all
for declaration in ('def generic', 'class generic'):
    for prefix in ('', '*', '**'):
        suffix = '()' if declaration.startswith('def') else ''
        ns = {}
        source = f'{declaration}[{prefix}tuple]{suffix}:\n    identity = tuple\n    def nested():\n        return tuple\n    values = [tuple for _ in (0,)]\n'
        if suffix:
            source += '    return identity, nested(), values[0]\n'
        exec(compile(source, 'identity.py', 'exec'), ns)
        generic = ns['generic']
        identities = generic() if suffix else (generic.identity, generic.nested(), generic.values[0])
        assert all(value is generic.__type_params__[0] for value in identities)

def redirected[tuple]():
    global tuple
    return tuple
assert redirected() is builtins.tuple

def enclosing():
    tuple = object()
    def captured[T]():
        nonlocal tuple
        return tuple
    assert captured() is tuple
enclosing()

# Reproduce the issue's observable failure when the destination is a TypeVar.
source = 'def broken[tuple]():\n    return list((1, 2))\nresult = broken()\n'
ns = {}
exec(compile(source, 'original.py', 'exec'), ns)
assert ns['result'] == [1, 2]
try:
    exec(compile(source.replace('list((1, 2))', 'tuple((1, 2))'), 'mutant.py', 'exec'), {})
except TypeError as error:
    assert 'TypeVar' in str(error) and 'not callable' in str(error)
else:
    raise AssertionError('mutant must call the non-builtin TypeVar')

# A class directive governs direct class lookup; descendants still capture
# the enclosing type parameter unless they declare their own global.
class ClassGlobal[tuple]:
    global tuple
    direct = tuple
    def method(self):
        return tuple
    def generic[T](self):
        return tuple
    via_lambda = lambda self: tuple
    values = [tuple for _ in range(1)]
    deferred = (tuple for _ in range(1))
    def explicit_global(self):
        global tuple
        return tuple
parameter = ClassGlobal.__type_params__[0]
assert ClassGlobal.direct is builtins.tuple
assert ClassGlobal().explicit_global() is builtins.tuple
assert ClassGlobal().method() is parameter
assert ClassGlobal().generic() is parameter
assert ClassGlobal().via_lambda() is parameter
assert ClassGlobal.values[0] is parameter
assert next(ClassGlobal.deferred) is parameter

def ordinary_closure():
    tuple = object()
    class C:
        global tuple
        direct = tuple
        def method(self):
            return tuple
    assert C.direct is builtins.tuple
    assert C().method() is tuple
ordinary_closure()

print('header/body/lazy identities OK')
