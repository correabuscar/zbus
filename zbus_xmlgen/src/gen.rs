use snakecase::ascii::to_snakecase;
use std::fmt::{Display, Formatter};

use zbus::quick_xml::{Arg, ArgDirection, Interface};
use zvariant::{
    Basic, ObjectPath, Signature, ARRAY_SIGNATURE_CHAR, DICT_ENTRY_SIG_END_CHAR,
    DICT_ENTRY_SIG_START_CHAR, STRUCT_SIG_END_CHAR, STRUCT_SIG_START_CHAR, VARIANT_SIGNATURE_CHAR,
};

pub struct GenTrait<'i>(pub &'i Interface<'i>);

impl<'i> Display for GenTrait<'i> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let iface = self.0;
        let idx = iface.name().rfind('.').unwrap() + 1;
        let name = &iface.name()[idx..];

        writeln!(f, "#[dbus_proxy(interface = \"{}\")]", iface.name())?;
        writeln!(f, "trait {} {{", name)?;

        let mut methods = iface.methods().to_vec();
        methods.sort_by(|a, b| a.name().partial_cmp(&b.name()).unwrap());
        for m in &methods {
            let (inputs, output) = inputs_output_from_args(m.args());
            let name = to_identifier(&to_snakecase(m.name().as_str()));
            writeln!(f)?;
            writeln!(f, "    /// {} method", m.name())?;
            if pascal_case(&name) != m.name().as_str() {
                writeln!(f, "    #[dbus_proxy(name = \"{}\")]", m.name())?;
            }
            writeln!(
                f,
                "    fn {name}({inputs}){output};",
                name = name,
                inputs = inputs,
                output = output
            )?;
        }

        let mut signals = iface.signals().to_vec();
        signals.sort_by(|a, b| a.name().partial_cmp(&b.name()).unwrap());
        for signal in &signals {
            let args = parse_signal_args(signal.args());
            let name = to_identifier(&to_snakecase(signal.name().as_str()));
            writeln!(f)?;
            writeln!(f, "    /// {} signal", signal.name())?;
            if pascal_case(&name) != signal.name().as_str() {
                writeln!(f, "    #[dbus_proxy(signal, name = \"{}\")]", signal.name())?;
            } else {
                writeln!(f, "    #[dbus_proxy(signal)]")?;
            }
            writeln!(
                f,
                "    fn {name}({args}) -> zbus::Result<()>;",
                name = name,
                args = args,
            )?;
        }

        let mut props = iface.properties().to_vec();
        props.sort_by(|a, b| a.name().partial_cmp(&b.name()).unwrap());
        for p in props {
            let name = to_identifier(&to_snakecase(p.name().as_str()));

            writeln!(f)?;
            writeln!(f, "    /// {} property", p.name())?;
            if pascal_case(&name) != p.name().as_str() {
                writeln!(f, "    #[dbus_proxy(property, name = \"{}\")]", p.name())?;
            } else {
                writeln!(f, "    #[dbus_proxy(property)]")?;
            }

            if p.access().read() {
                let output = to_rust_type(p.ty(), false, false);
                writeln!(
                    f,
                    "    fn {name}(&self) -> zbus::Result<{output}>;",
                    name = name,
                    output = output,
                )?;
            }

            if p.access().write() {
                let input = to_rust_type(p.ty(), true, true);
                writeln!(
                    f,
                    "    fn set_{name}(&self, value: {input}) -> zbus::Result<()>;",
                    name = name,
                    input = input,
                )?;
            }
        }
        writeln!(f, "}}")
    }
}

fn inputs_output_from_args(args: &[Arg]) -> (String, String) {
    let mut inputs = vec!["&self".to_string()];
    let mut output = vec![];
    let mut n = 0;
    let mut gen_name = || {
        n += 1;
        format!("arg_{}", n)
    };

    for a in args {
        match a.direction() {
            None | Some(ArgDirection::In) => {
                let ty = to_rust_type(a.ty(), true, true);
                let arg = if let Some(name) = a.name() {
                    to_identifier(name)
                } else {
                    gen_name()
                };
                inputs.push(format!("{}: {}", arg, ty));
            }
            Some(ArgDirection::Out) => {
                let ty = to_rust_type(a.ty(), false, false);
                output.push(ty);
            }
        }
    }

    let output = match output.len() {
        0 => "()".to_string(),
        1 => output[0].to_string(),
        _ => format!("({})", output.join(", ")),
    };

    (inputs.join(", "), format!(" -> zbus::Result<{}>", output))
}

fn parse_signal_args(args: &[Arg]) -> String {
    let mut inputs = vec!["&self".to_string()];
    let mut n = 0;
    let mut gen_name = || {
        n += 1;
        format!("arg_{}", n)
    };

    for a in args {
        let ty = to_rust_type(a.ty(), true, false);
        let arg = if let Some(name) = a.name() {
            to_identifier(name)
        } else {
            gen_name()
        };
        inputs.push(format!("{}: {}", arg, ty));
    }

    inputs.join(", ")
}

fn to_rust_type(ty: &str, input: bool, as_ref: bool) -> String {
    // can't haz recursive closure, yet
    fn iter_to_rust_type(
        it: &mut std::iter::Peekable<std::slice::Iter<'_, u8>>,
        input: bool,
        as_ref: bool,
    ) -> String {
        let c = it.next().unwrap();
        match *c as char {
            u8::SIGNATURE_CHAR => "u8".into(),
            bool::SIGNATURE_CHAR => "bool".into(),
            i16::SIGNATURE_CHAR => "i16".into(),
            u16::SIGNATURE_CHAR => "u16".into(),
            i32::SIGNATURE_CHAR => "i32".into(),
            u32::SIGNATURE_CHAR => "u32".into(),
            i64::SIGNATURE_CHAR => "i64".into(),
            u64::SIGNATURE_CHAR => "u64".into(),
            f64::SIGNATURE_CHAR => "f64".into(),
            // xmlgen accepts 'h' on Windows, only for code generation
            'h' => (if input {
                "zbus::zvariant::Fd"
            } else {
                "zbus::zvariant::OwnedFd"
            })
            .into(),
            <&str>::SIGNATURE_CHAR => (if input || as_ref { "&str" } else { "String" }).into(),
            ObjectPath::SIGNATURE_CHAR => (if input {
                if as_ref {
                    "&zbus::zvariant::ObjectPath<'_>"
                } else {
                    "zbus::zvariant::ObjectPath<'_>"
                }
            } else {
                "zbus::zvariant::OwnedObjectPath"
            })
            .into(),
            Signature::SIGNATURE_CHAR => (if input {
                if as_ref {
                    "&zbus::zvariant::Signature<'_>"
                } else {
                    "zbus::zvariant::Signature<'_>"
                }
            } else {
                "zbus::zvariant::OwnedSignature"
            })
            .into(),
            VARIANT_SIGNATURE_CHAR => (if input {
                if as_ref {
                    "&zbus::zvariant::Value<'_>"
                } else {
                    "zbus::zvariant::Value<'_>"
                }
            } else {
                "zbus::zvariant::OwnedValue"
            })
            .into(),
            ARRAY_SIGNATURE_CHAR => {
                let c = it.peek().unwrap();
                match **c as char {
                    '{' => format!(
                        "std::collections::HashMap<{}>",
                        iter_to_rust_type(it, input, false)
                    ),
                    _ => {
                        let ty = iter_to_rust_type(it, input, false);
                        if input {
                            format!("&[{}]", ty)
                        } else {
                            format!("{}Vec<{}>", if as_ref { "&" } else { "" }, ty)
                        }
                    }
                }
            }
            c @ STRUCT_SIG_START_CHAR | c @ DICT_ENTRY_SIG_START_CHAR => {
                let dict = c == '{';
                let mut vec = vec![];
                loop {
                    let c = it.peek().unwrap();
                    match **c as char {
                        STRUCT_SIG_END_CHAR | DICT_ENTRY_SIG_END_CHAR => break,
                        _ => vec.push(iter_to_rust_type(it, input, false)),
                    }
                }
                if dict {
                    vec.join(", ")
                } else if vec.len() > 1 {
                    format!("{}({})", if as_ref { "&" } else { "" }, vec.join(", "))
                } else {
                    vec[0].to_string()
                }
            }
            _ => unimplemented!(),
        }
    }

    let mut it = ty.as_bytes().iter().peekable();
    iter_to_rust_type(&mut it, input, as_ref)
}

static KWORDS: &[&str] = &[
    "Self", "abstract", "as", "async", "await", "become", "box", "break", "const", "continue",
    "crate", "do", "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "if", "impl",
    "in", "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref",
    "return", "self", "static", "struct", "super", "trait", "true", "try", "type", "typeof",
    "union", "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
];

fn to_identifier(id: &str) -> String {
    if KWORDS.contains(&id) {
        format!("{}_", id)
    } else {
        id.replace('-', "_")
    }
}

// This function is the same as zbus_macros::utils::pascal_case
pub fn pascal_case(s: &str) -> String {
    let mut pascal = String::new();
    let mut capitalize = true;
    for ch in s.chars() {
        if ch == '_' {
            capitalize = true;
        } else if capitalize {
            pascal.push(ch.to_ascii_uppercase());
            capitalize = false;
        } else {
            pascal.push(ch);
        }
    }
    pascal
}

#[cfg(test)]
mod tests {
    use std::{error::Error, result::Result};

    use super::GenTrait;
    use zbus::quick_xml::Node;

    static EXAMPLE: &str = r##"
<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN"
  "http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd">
 <node name="/com/example/sample_object0">
   <interface name="com.example.SampleInterface0">
     <method name="Frobate">
       <arg name="foz" type="i"/>
       <arg name="foo" type="i" direction="in"/>
       <arg name="bar" type="s" direction="out"/>
       <arg name="baz" type="a{us}" direction="out"/>
       <annotation name="org.freedesktop.DBus.Deprecated" value="true"/>
     </method>
     <method name="Bazify">
       <arg name="bar" type="(iiu)" direction="in"/>
       <arg name="bar" type="v" direction="out"/>
     </method>
     <method name="MogrifyMe">
       <arg name="bar" type="(iiav)" direction="in"/>
     </method>
     <signal name="Changed">
       <arg name="new_value" type="b"/>
     </signal>
     <signal name="Changed2">
       <arg name="new_value" type="b" direction="out"/>
       <arg name="new_value2" type="b" direction="out"/>
     </signal>
     <property name="Bar" type="y" access="readwrite"/>
   </interface>
   <node name="child_of_sample_object"/>
   <node name="another_child_of_sample_object"/>
</node>
"##;

    #[test]
    fn gen() -> Result<(), Box<dyn Error>> {
        let node = Node::from_reader(EXAMPLE.as_bytes())?;
        let t = format!("{}", GenTrait(&node.interfaces()[0]));
        println!("{}", t);
        Ok(())
    }
}
