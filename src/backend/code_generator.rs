use std::collections::{BTreeSet, HashMap};
use std::mem;
use std::rc::Rc;

use crate::ast::token::{Token, TokenType};
use crate::ast::expr::{Expr, MatchCase, Pattern};
use crate::ast::stmt::{EnumVariant, FunctionStmt, Stmt, VariableBinding};
use crate::error::CodegenError;
use crate::type_system::types::TypeExpr;
use crate::type_system::type_checker::{BuiltinCallType, TypeInfo};

fn mangle(name: &str) -> String {
    format!("_delo_{name}")
}

fn is_primitive_c_type(c_type: &str) -> bool {
    matches!(c_type, "int" | "double" | "bool" | "const char*" | "void")
}

fn default_value_for_c_type(c_type: &str) -> &'static str {
    match c_type {
        "int" => "0",
        "double" => "0.0",
        "bool" => "false",
        "const char*" => "\"\"",
        _ => "{0}",
    }
}

enum PrimitivePatternType {
    CatchAll { binding: Option<(String, String)> },
    Conditional { condition: String },
}

struct ExprCode {
    pre_expr_stmts: Vec<String>,
    expr: String,
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
struct UserTypeInstantiation {
    identifier: String,
    argument_c_types: Vec<String>,
}

impl UserTypeInstantiation {
    fn c_identifier(&self) -> String {
        let mut parts = vec![mangle(&self.identifier)];
        for c_argument in &self.argument_c_types {
            parts.push(Self::suffix(&c_argument));
        }
        parts.join("_")
    }

    fn suffix(c_type: &str) -> String {
        c_type.strip_prefix("struct ")
            .unwrap_or(c_type)
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect()
    }
}

#[derive(Eq, PartialEq, Ord, PartialOrd)]
struct FunctionInstantiation {
    base_identifier: String,
    argument_c_types: Vec<String>,
}

impl FunctionInstantiation {
    fn c_identifier(&self) -> String {
        let mut parts = vec![mangle(&self.base_identifier)];
        for t in &self.argument_c_types {
            parts.push(UserTypeInstantiation::suffix(t));
        }
        parts.join("_")
    }
}

fn tuple_c_type_name(element_c_types: &[String]) -> String {
    let mut parts = vec!["Tuple".to_string()];
    for c in element_c_types {
        parts.push(UserTypeInstantiation::suffix(c));
    }
    parts.join("_")
}

pub struct CodeGenerator<'a> {
    types: &'a TypeInfo,
    output: String,
    indent_level: usize,
    temp_id_counter: usize,
    array_instantiations: BTreeSet<String>,
    range_instantiations: BTreeSet<String>,
    map_instantiations: BTreeSet<(String, String)>,
    array_map_instantiations: BTreeSet<(String, String)>,
    array_filter_instantiations: BTreeSet<String>,
    array_foldl_instantiations: BTreeSet<(String, String)>,
    array_foldr_instantiations: BTreeSet<(String, String)>,
    tuple_instantiations: BTreeSet<Vec<String>>,
    history_instantiations: BTreeSet<String>,
    user_type_instantiations: BTreeSet<UserTypeInstantiation>,
    function_instantiations: BTreeSet<FunctionInstantiation>,
    ast_structs: HashMap<String, Stmt>,
    ast_enums: HashMap<String, Stmt>,
    ast_functions: HashMap<String, Rc<FunctionStmt>>,
}

impl<'a> CodeGenerator<'a> {
    pub fn new(types: &'a TypeInfo) -> Self {
        Self {
            types,
            output: String::new(),
            indent_level: 0,
            temp_id_counter: 0,
            array_instantiations: BTreeSet::new(),
            map_instantiations: BTreeSet::new(),
            array_map_instantiations: BTreeSet::new(),
            array_filter_instantiations: BTreeSet::new(),
            array_foldl_instantiations: BTreeSet::new(),
            array_foldr_instantiations: BTreeSet::new(),
            range_instantiations: BTreeSet::new(),
            tuple_instantiations: BTreeSet::new(),
            history_instantiations: BTreeSet::new(),
            user_type_instantiations: BTreeSet::new(),
            function_instantiations: BTreeSet::new(),
            ast_structs: HashMap::new(),
            ast_enums: HashMap::new(),
            ast_functions: HashMap::new(),
        }
    }

    pub fn generate_program(&mut self, stmts: &[Stmt]) -> Result<String, CodegenError> {
        self.write_raw(r#"
            #include <stdio.h>
            #include <stdlib.h>
            #include <stdbool.h>
            #include <string.h>
            #include <stddef.h>
            #include <stdint.h>
            #include <math.h>
            #include <setjmp.h>
            #ifdef _WIN32
            #include <windows.h>
            #include <bcrypt.h>
            #else
            #include <fcntl.h>
            #include <unistd.h>
            #endif

        "#);
        self.write_time_travel_runtime();

        for stmt in stmts {
            self.generate_declarations(stmt);
        }

        self.write_raw(r#"
            typedef struct Fn {
                void* env;
                void* fn;
            } Fn;

        "#);

        self.write_helper_functions();

        let mut main_output = String::new();
        mem::swap(&mut self.output, &mut main_output);
        self.generate_main(stmts)?;
        mem::swap(&mut self.output, &mut main_output);

        self.write_array_types();
        self.write_array_higher_order_helpers();
        self.write_range_types();
        self.write_user_type_instantiations();
        self.write_tuple_types();
        self.write_map_types();
        self.write_history_types();
        self.emit_tracked_var_globals(stmts)?;
        self.write_tuple_helper_declarations();
        self.write_function_instantiations()?;
        self.write_format_helpers();
        self.write_tuple_helpers();

        self.output.push_str(&main_output);

        Ok(self.output.clone())
    }

    fn write_time_travel_runtime(&mut self) {
        self.write_raw(r#"
            // ===== Time-travel runtime =====
            typedef struct {
                size_t* call_path;
                size_t call_path_len;
                size_t local_event;
            } DeloEventId;

            #define DELO_MAX_DEPTH 1024
            static size_t delo_call_stack[DELO_MAX_DEPTH];
            static size_t delo_local_events[DELO_MAX_DEPTH];
            static size_t delo_stack_depth = 0;
            static size_t delo_next_call_id = 1;
            static bool delo_replay_mode = false;
            static DeloEventId delo_resume_event;
            static jmp_buf delo_restart_env;
            static bool delo_init_done = false;

            typedef struct {
                void* history;
                void (*reset_cursor)(void*);
                void (*truncate)(void*);
            } DeloHistoryHandle;
            static DeloHistoryHandle* delo_histories = NULL;
            static size_t delo_histories_len = 0;
            static size_t delo_histories_cap = 0;

            static void delo_register_history(void* h, void (*reset_cursor)(void*), void (*truncate)(void*)) {
                if (delo_histories_len >= delo_histories_cap) {
                    delo_histories_cap = delo_histories_cap == 0 ? 16 : delo_histories_cap * 2;
                    delo_histories = (DeloHistoryHandle*)realloc(delo_histories, sizeof(DeloHistoryHandle) * delo_histories_cap);
                }
                delo_histories[delo_histories_len].history = h;
                delo_histories[delo_histories_len].reset_cursor = reset_cursor;
                delo_histories[delo_histories_len].truncate = truncate;
                delo_histories_len++;
            }

            static void delo_reset_all_cursors(void) {
                for (size_t i = 0; i < delo_histories_len; i++) {
                    delo_histories[i].reset_cursor(delo_histories[i].history);
                }
            }

            static void delo_truncate_all(void) {
                for (size_t i = 0; i < delo_histories_len; i++) {
                    delo_histories[i].truncate(delo_histories[i].history);
                }
            }

            static void delo_call_push(void) {
                delo_call_stack[delo_stack_depth] = delo_next_call_id++;
                delo_local_events[delo_stack_depth] = 0;
                delo_stack_depth++;
            }

            static void delo_call_pop(void) {
                delo_stack_depth--;
            }

            static DeloEventId delo_capture_event_id(void) {
                DeloEventId id;
                id.call_path = (size_t*)malloc(sizeof(size_t) * delo_stack_depth);
                memcpy(id.call_path, delo_call_stack, sizeof(size_t) * delo_stack_depth);
                id.call_path_len = delo_stack_depth;
                id.local_event = delo_local_events[delo_stack_depth - 1];
                return id;
            }

            static bool delo_event_id_eq(DeloEventId a, DeloEventId b) {
                if (a.call_path_len != b.call_path_len) return false;
                if (a.local_event != b.local_event) return false;
                for (size_t i = 0; i < a.call_path_len; i++) {
                    if (a.call_path[i] != b.call_path[i]) return false;
                }
                return true;
            }

            static bool delo_at_resume_event(DeloEventId current) {
                return delo_event_id_eq(current, delo_resume_event);
            }

            static void delo_increment_local_event(void) {
                delo_local_events[delo_stack_depth - 1]++;
            }

            static void delo_trigger_replay(DeloEventId resume_event) {
                delo_resume_event = resume_event;
                delo_replay_mode = true;
                delo_stack_depth = 1;
                delo_call_stack[0] = 1;
                delo_local_events[0] = 0;
                delo_next_call_id = 2;
                delo_reset_all_cursors();
                longjmp(delo_restart_env, 1);
            }

        "#);
    }

    fn write_helper_functions(&mut self) {
        self.write_string_concat_helper();
        self.write_string_repeat_helper();
        self.write_int_pow_helper();
        self.write_hash_helpers();
        self.write_format_primitives();
    }

    fn write_hash_helpers(&mut self) {
        self.write_raw(r#"
            // SipHash-1-3 (https://github.com/veorq/SipHash)
            static uint8_t delo_siphash_key[16];

            static void delo_init_siphash_key(void) {
            #ifdef _WIN32
                if (BCryptGenRandom(NULL, delo_siphash_key, 16, BCRYPT_USE_SYSTEM_PREFERRED_RNG) != 0) {
                    fprintf(stderr, "Failed to obtain random bytes for SipHash key\n");
                    exit(1);
                }
            #else
                int fd = open("/dev/urandom", O_RDONLY);
                if (fd < 0) {
                    fprintf(stderr, "Failed to open /dev/urandom for SipHash key\n");
                    exit(1);
                }
                size_t total = 0;
                while (total < 16) {
                    ssize_t got = read(fd, delo_siphash_key + total, 16 - total);
                    if (got <= 0) {
                        close(fd);
                        fprintf(stderr, "Failed to read random bytes for SipHash key\n");
                        exit(1);
                    }
                    total += (size_t)got;
                }
                close(fd);
            #endif
            }

            #define ROTL(x, b) (uint64_t)(((x) << (b)) | ((x) >> (64 - (b))))

            #define U8TO64_LE(p)                                                           \
                (((uint64_t)((p)[0])) | ((uint64_t)((p)[1]) << 8) |                        \
                 ((uint64_t)((p)[2]) << 16) | ((uint64_t)((p)[3]) << 24) |                 \
                 ((uint64_t)((p)[4]) << 32) | ((uint64_t)((p)[5]) << 40) |                 \
                 ((uint64_t)((p)[6]) << 48) | ((uint64_t)((p)[7]) << 56))

            #define SIPROUND                                                               \
                do {                                                                       \
                    v0 += v1;                                                              \
                    v1 = ROTL(v1, 13);                                                     \
                    v1 ^= v0;                                                              \
                    v0 = ROTL(v0, 32);                                                     \
                    v2 += v3;                                                              \
                    v3 = ROTL(v3, 16);                                                     \
                    v3 ^= v2;                                                              \
                    v0 += v3;                                                              \
                    v3 = ROTL(v3, 21);                                                     \
                    v3 ^= v0;                                                              \
                    v2 += v1;                                                              \
                    v1 = ROTL(v1, 17);                                                     \
                    v1 ^= v2;                                                              \
                    v2 = ROTL(v2, 32);                                                     \
                } while (0)

            size_t delo_siphash13(const void *in, const size_t inlen) {
                const unsigned char *ni = (const unsigned char *)in;
                const unsigned char *kk = (const unsigned char *)delo_siphash_key;

                uint64_t v0 = UINT64_C(0x736f6d6570736575);
                uint64_t v1 = UINT64_C(0x646f72616e646f6d);
                uint64_t v2 = UINT64_C(0x6c7967656e657261);
                uint64_t v3 = UINT64_C(0x7465646279746573);
                uint64_t k0 = U8TO64_LE(kk);
                uint64_t k1 = U8TO64_LE(kk + 8);
                uint64_t m;
                const unsigned char *end = ni + inlen - (inlen % sizeof(uint64_t));
                const int left = inlen & 7;
                uint64_t b = ((uint64_t)inlen) << 56;
                v3 ^= k1;
                v2 ^= k0;
                v1 ^= k1;
                v0 ^= k0;

                for (; ni != end; ni += 8) {
                    m = U8TO64_LE(ni);
                    v3 ^= m;

                    SIPROUND;

                    v0 ^= m;
                }

                switch (left) {
                case 7:
                    b |= ((uint64_t)ni[6]) << 48;
                    /* FALLTHRU */
                case 6:
                    b |= ((uint64_t)ni[5]) << 40;
                    /* FALLTHRU */
                case 5:
                    b |= ((uint64_t)ni[4]) << 32;
                    /* FALLTHRU */
                case 4:
                    b |= ((uint64_t)ni[3]) << 24;
                    /* FALLTHRU */
                case 3:
                    b |= ((uint64_t)ni[2]) << 16;
                    /* FALLTHRU */
                case 2:
                    b |= ((uint64_t)ni[1]) << 8;
                    /* FALLTHRU */
                case 1:
                    b |= ((uint64_t)ni[0]);
                    break;
                case 0:
                    break;
                }

                v3 ^= b;

                SIPROUND;

                v0 ^= b;

                v2 ^= 0xff;

                SIPROUND;
                SIPROUND;
                SIPROUND;

                b = v0 ^ v1 ^ v2 ^ v3;
                return (size_t)b;
            }

            #undef SIPROUND
            #undef U8TO64_LE
            #undef ROTL

            size_t int_hash(uint64_t x) {
                return delo_siphash13(&x, sizeof(x));
            }

            size_t double_hash(double x) {
                return delo_siphash13(&x, sizeof(x));
            }

            size_t string_hash(const char* s) {
                return delo_siphash13(s, strlen(s));
            }

        "#);
    }

    fn write_string_repeat_helper(&mut self) {
        self.write_raw(r#"
            const char* string_repeat(const char* s, int n) {
                if (n <= 0) {
                    char* empty = (char*)malloc(1);
                    if (!empty) { fprintf(stderr, "Out of memory in string_repeat\n"); exit(1); }
                    empty[0] = '\0';
                    return empty;
                }
                size_t len = strlen(s);
                size_t total = len * (size_t)n;
                char* result = (char*)malloc(total + 1);
                if (!result) { fprintf(stderr, "Out of memory in string_repeat\n"); exit(1); }
                for (int i = 0; i < n; ++i) { memcpy(result + i * len, s, len); }
                result[total] = '\0';
                return result;
            }

        "#);
    }

    fn write_int_pow_helper(&mut self) {
        self.write_raw(r#"
            int int_pow(int base, int exp) {
                int result = 1;
                if (exp < 0) { return 0; }
                while (exp > 0) {
                    if (exp & 1) { result *= base; }
                    base *= base;
                    exp >>= 1;
                }
                return result;
            }

        "#);
    }

    fn write_string_concat_helper(&mut self) {
        self.write_raw(r#"
            const char* string_concat(const char* a, const char* b) {
                size_t len_a = strlen(a);
                size_t len_b = strlen(b);
                char* result = (char*)malloc(len_a + len_b + 1);
                if (!result) { fprintf(stderr, "Out of memory in string_concat\n"); exit(1); }
                memcpy(result, a, len_a);
                memcpy(result + len_a, b, len_b);
                result[len_a + len_b] = '\0';
                return result;
            }

        "#);
    }

    fn write_format_primitives(&mut self) {
        self.write_raw(r#"
            void format_int(int x, FILE* stream) {
                fprintf(stream, "%d", x);
            }

            void format_double(double x, FILE* stream) {
                fprintf(stream, "%g", x);
            }

            void format_bool(bool b, FILE* stream) {
                fprintf(stream, "%s", b ? "True" : "False");
            }

            void format_string(const char* s, FILE* stream) {
                fprintf(stream, "%s", s);
            }

            void format_Fn(Fn f, FILE* stream) {
                fprintf(stream, "<function at %p>", f.fn);
            }

        "#);
    }

    fn write_array_types(&mut self) {
        let array_instantiations = self.array_instantiations.clone();
        for array_instantiation in array_instantiations {
            let suffix = UserTypeInstantiation::suffix(&array_instantiation);
            self.write_line(&format!("typedef struct {{ size_t length; size_t capacity; {array_instantiation}* data; }} Array_{suffix};"));
            self.write_line(&format!("Array_{suffix} make_Array_{suffix}(size_t length, {array_instantiation} const* elems) {{"));
            self.indent();
            self.write_line(&format!("Array_{suffix} arr; arr.length = length; arr.capacity = length;"));
            self.write_line(&format!("arr.data = ({array_instantiation}*)malloc(sizeof({array_instantiation}) * length);"));
            self.write_line("for (size_t i = 0; i < length; ++i) { arr.data[i] = elems[i]; }");
            self.write_line("return arr;");
            self.un_indent();
            self.write_line("}");
            self.write_line("");

            self.write_line(&format!("Array_{suffix} array_concat_{suffix}(Array_{suffix} a, Array_{suffix} b) {{"));
            self.indent();
            self.write_line("size_t total = a.length + b.length;");
            self.write_line(&format!("Array_{suffix} result;"));
            self.write_line("result.length = total;");
            self.write_line("result.capacity = total;");
            self.write_line(&format!("result.data = ({array_instantiation}*)malloc(sizeof({array_instantiation}) * total);"));
            self.write_line("if (!result.data) { fprintf(stderr, \"Out of memory in array_concat\\n\"); exit(1); }");
            self.write_line("for (size_t i = 0; i < a.length; ++i) { result.data[i] = a.data[i]; }");
            self.write_line("for (size_t i = 0; i < b.length; ++i) { result.data[a.length + i] = b.data[i]; }");
            self.write_line("return result;");
            self.un_indent();
            self.write_line("}");
            self.write_line("");

            self.write_line(&format!("Array_{suffix} array_repeat_{suffix}(Array_{suffix} a, int n) {{"));
            self.indent();
            self.write_line(&format!("Array_{suffix} result;"));
            self.write_line("if (n <= 0) { result.length = 0; result.capacity = 0; result.data = NULL; return result; }");
            self.write_line("size_t total = a.length * (size_t)n;");
            self.write_line("result.length = total;");
            self.write_line("result.capacity = total;");
            self.write_line(&format!("result.data = ({array_instantiation}*)malloc(sizeof({array_instantiation}) * total);"));
            self.write_line("if (!result.data) { fprintf(stderr, \"Out of memory in array_repeat\\n\"); exit(1); }");
            self.write_line("for (int rep = 0; rep < n; ++rep) {");
            self.indent();
            self.write_line("for (size_t i = 0; i < a.length; ++i) { result.data[(size_t)rep * a.length + i] = a.data[i]; }");
            self.un_indent();
            self.write_line("}");
            self.write_line("return result;");
            self.un_indent();
            self.write_line("}");
            self.write_line("");
        }
    }

    fn write_map_types(&mut self) {
        let map_instantiations = self.map_instantiations.clone();
        for (key_c_type, value_c_type) in map_instantiations {
            let key_suffix = UserTypeInstantiation::suffix(&key_c_type);
            let value_suffix = UserTypeInstantiation::suffix(&value_c_type);
            let suffix = format!("{key_suffix}_{value_suffix}");
            let map_type_name = format!("Map_{suffix}");
            let slot_type_name = format!("{map_type_name}_Slot");

            let key_eq = if key_c_type == "const char*" {
                "strcmp(a, b) == 0"
            } else {
                "a == b"
            };

            let key_hash = match key_c_type.as_str() {
                "const char*" => "string_hash(key)".to_string(),
                "double" => "double_hash(key)".to_string(),
                _ => "int_hash((uint64_t)key)".to_string(),
            };

            self.write_line(&format!("typedef struct {{ bool occupied; {key_c_type} key; {value_c_type} value; }} {slot_type_name};"));
            self.write_line(&format!("typedef struct {{ size_t length; size_t capacity; {slot_type_name}* slots; }} {map_type_name};"));
            self.write_line("");

            self.write_line(&format!("void map_set_{suffix}({map_type_name}* m, {key_c_type} key, {value_c_type} value);"));
            self.write_line("");

            self.write_line(&format!("size_t map_probe_{suffix}({slot_type_name}* slots, size_t capacity, {key_c_type} key) {{"));
            self.indent();
            self.write_line(&format!("size_t mask = capacity - 1;"));
            self.write_line(&format!("size_t idx = {key_hash} & mask;"));
            self.write_line("while (slots[idx].occupied) {");
            self.indent();
            self.write_line(&format!("{key_c_type} a = slots[idx].key; {key_c_type} b = key;"));
            self.write_line(&format!("if ({key_eq}) {{ return idx; }}"));
            self.write_line("idx = (idx + 1) & mask;");
            self.un_indent();
            self.write_line("}");
            self.write_line("return idx;");
            self.un_indent();
            self.write_line("}");
            self.write_line("");

            self.write_line(&format!("void map_resize_{suffix}({map_type_name}* m, size_t new_capacity) {{"));
            self.indent();
            self.write_line(&format!("{slot_type_name}* old_slots = m->slots;"));
            self.write_line("size_t old_capacity = m->capacity;");
            self.write_line(&format!("{slot_type_name}* new_slots = ({slot_type_name}*)calloc(new_capacity, sizeof({slot_type_name}));"));
            self.write_line("if (!new_slots) { fprintf(stderr, \"Out of memory in map_resize\\n\"); exit(1); }");
            self.write_line("m->slots = new_slots;");
            self.write_line("m->capacity = new_capacity;");
            self.write_line("for (size_t i = 0; i < old_capacity; ++i) {");
            self.indent();
            self.write_line("if (!old_slots[i].occupied) { continue; }");
            self.write_line(&format!("size_t target = map_probe_{suffix}(new_slots, new_capacity, old_slots[i].key);"));
            self.write_line("new_slots[target].occupied = true;");
            self.write_line("new_slots[target].key = old_slots[i].key;");
            self.write_line("new_slots[target].value = old_slots[i].value;");
            self.un_indent();
            self.write_line("}");
            self.write_line("free(old_slots);");
            self.un_indent();
            self.write_line("}");
            self.write_line("");

            self.write_line(&format!("{value_c_type} map_get_{suffix}({map_type_name} m, {key_c_type} key) {{"));
            self.indent();
            self.write_line("if (m.capacity == 0) { fprintf(stderr, \"Map key not found\\n\"); exit(1); }");
            self.write_line(&format!("size_t idx = map_probe_{suffix}(m.slots, m.capacity, key);"));
            self.write_line("if (!m.slots[idx].occupied) { fprintf(stderr, \"Map key not found\\n\"); exit(1); }");
            self.write_line("return m.slots[idx].value;");
            self.un_indent();
            self.write_line("}");
            self.write_line("");

            self.write_line(&format!("void map_set_{suffix}({map_type_name}* m, {key_c_type} key, {value_c_type} value) {{"));
            self.indent();
            self.write_line("if (m->capacity == 0) {");
            self.indent();
            self.write_line(&format!("map_resize_{suffix}(m, 8);"));
            self.un_indent();
            self.write_line(&format!("}} else if ((m->length + 1) * 2 >= m->capacity) {{"));
            self.indent();
            self.write_line(&format!("map_resize_{suffix}(m, m->capacity * 2);"));
            self.un_indent();
            self.write_line("}");
            self.write_line(&format!("size_t idx = map_probe_{suffix}(m->slots, m->capacity, key);"));
            self.write_line("if (m->slots[idx].occupied) {");
            self.indent();
            self.write_line("m->slots[idx].value = value;");
            self.write_line("return;");
            self.un_indent();
            self.write_line("}");
            self.write_line("m->slots[idx].occupied = true;");
            self.write_line("m->slots[idx].key = key;");
            self.write_line("m->slots[idx].value = value;");
            self.write_line("++m->length;");
            self.un_indent();
            self.write_line("}");
            self.write_line("");

            self.write_line(&format!("{map_type_name} make_{map_type_name}(size_t count, {key_c_type} const* in_keys, {value_c_type} const* in_values) {{"));
            self.indent();
            self.write_line(&format!("{map_type_name} m;"));
            self.write_line("m.length = 0;");
            self.write_line("m.capacity = 0;");
            self.write_line("m.slots = NULL;");
            self.write_line("for (size_t i = 0; i < count; ++i) {");
            self.indent();
            self.write_line(&format!("map_set_{suffix}(&m, in_keys[i], in_values[i]);"));
            self.un_indent();
            self.write_line("}");
            self.write_line("return m;");
            self.un_indent();
            self.write_line("}");
            self.write_line("");
        }
    }

    fn write_array_higher_order_helpers(&mut self) {
        let map_instantiations = std::mem::take(&mut self.array_map_instantiations);
        for (t_c_type, u_c_type) in map_instantiations {
            let t_suffix = UserTypeInstantiation::suffix(&t_c_type);
            let u_suffix = UserTypeInstantiation::suffix(&u_c_type);
            self.write_raw(&format!(r#"
                Array_{u_suffix} array_map_{t_suffix}_{u_suffix}(Array_{t_suffix} arr, Fn f) {{
                    Array_{u_suffix} result;
                    result.length = arr.length;
                    result.capacity = arr.length;
                    result.data = ({u_c_type}*)malloc(sizeof({u_c_type}) * (arr.length > 0 ? arr.length : 1));
                    if (!result.data && arr.length > 0) {{ fprintf(stderr, "Out of memory in array_map\n"); exit(1); }}
                    for (size_t i = 0; i < arr.length; ++i) {{
                        delo_call_push();
                        result.data[i] = (({u_c_type} (*)(void*, {t_c_type}))(f.fn))(f.env, arr.data[i]);
                        delo_call_pop();
                    }}
                    return result;
                }}

            "#));
        }

        let filter_instantiations = std::mem::take(&mut self.array_filter_instantiations);
        for t_c_type in filter_instantiations {
            let t_suffix = UserTypeInstantiation::suffix(&t_c_type);
            self.write_raw(&format!(r#"
                Array_{t_suffix} array_filter_{t_suffix}(Array_{t_suffix} arr, Fn f) {{
                    Array_{t_suffix} result;
                    result.length = 0;
                    result.capacity = arr.length > 0 ? arr.length : 1;
                    result.data = ({t_c_type}*)malloc(sizeof({t_c_type}) * result.capacity);
                    if (!result.data) {{ fprintf(stderr, "Out of memory in array_filter\n"); exit(1); }}
                    for (size_t i = 0; i < arr.length; ++i) {{
                        delo_call_push();
                        bool keep = ((bool (*)(void*, {t_c_type}))(f.fn))(f.env, arr.data[i]);
                        delo_call_pop();
                        if (keep) {{
                            result.data[result.length++] = arr.data[i];
                        }}
                    }}
                    return result;
                }}

            "#));
        }

        let foldl_instantiations = std::mem::take(&mut self.array_foldl_instantiations);
        for (t_c_type, u_c_type) in foldl_instantiations {
            let t_suffix = UserTypeInstantiation::suffix(&t_c_type);
            let u_suffix = UserTypeInstantiation::suffix(&u_c_type);
            self.write_raw(&format!(r#"
                {u_c_type} array_foldl_{t_suffix}_{u_suffix}(Array_{t_suffix} arr, {u_c_type} init, Fn f) {{
                    {u_c_type} acc = init;
                    for (size_t i = 0; i < arr.length; ++i) {{
                        delo_call_push();
                        acc = (({u_c_type} (*)(void*, {u_c_type}, {t_c_type}))(f.fn))(f.env, acc, arr.data[i]);
                        delo_call_pop();
                    }}
                    return acc;
                }}

            "#));
        }

        let foldr_instantiations = std::mem::take(&mut self.array_foldr_instantiations);
        for (t_c_type, u_c_type) in foldr_instantiations {
            let t_suffix = UserTypeInstantiation::suffix(&t_c_type);
            let u_suffix = UserTypeInstantiation::suffix(&u_c_type);
            self.write_raw(&format!(r#"
                {u_c_type} array_foldr_{t_suffix}_{u_suffix}(Array_{t_suffix} arr, {u_c_type} init, Fn f) {{
                    {u_c_type} acc = init;
                    for (size_t i = arr.length; i > 0; --i) {{
                        delo_call_push();
                        acc = (({u_c_type} (*)(void*, {u_c_type}, {t_c_type}))(f.fn))(f.env, acc, arr.data[i - 1]);
                        delo_call_pop();
                    }}
                    return acc;
                }}

            "#));
        }
    }

    fn write_range_types(&mut self) {
        let range_instantiations = self.range_instantiations.clone();
        for range_instantiation in range_instantiations {
            let suffix = UserTypeInstantiation::suffix(&range_instantiation);
            self.write_line(&format!("typedef struct {{ {range_instantiation} start; {range_instantiation} end; bool inclusive; }} Range_{suffix};"));
            self.write_line(&format!("Range_{suffix} make_Range_{suffix}({range_instantiation} start, {range_instantiation} end, bool inclusive) {{"));
            self.indent();
            self.write_line(&format!("Range_{suffix} r; r.start = start; r.end = end; r.inclusive = inclusive;"));
            self.write_line("return r;");
            self.un_indent();
            self.write_line("}");
            self.write_line("");
        }
    }

    fn write_tuple_types(&mut self) {
        let mut remaining: Vec<Vec<String>> = self.tuple_instantiations.iter().cloned().collect();
        let mut emitted: BTreeSet<String> = BTreeSet::new();

        while !remaining.is_empty() {
            let mut progressed = false;
            let mut still_blocked: Vec<Vec<String>> = Vec::new();

            for elements in remaining.into_iter() {
                let deps_ready = elements.iter().all(|c| !c.starts_with("Tuple_") || emitted.contains(c));
                if deps_ready {
                    let name = tuple_c_type_name(&elements);
                    self.write_line(&format!("typedef struct {name} {{"));
                    self.indent();
                    for (i, c_type) in elements.iter().enumerate() {
                        self.write_line(&format!("{c_type} _{i};"));
                    }
                    self.un_indent();
                    self.write_line(&format!("}} {name};"));
                    self.write_line("");
                    emitted.insert(name);
                    progressed = true;
                } else {
                    still_blocked.push(elements);
                }
            }

            if !progressed {
                break;
            }
            remaining = still_blocked;
        }
    }

    fn write_history_types(&mut self) {
        let history_instantiations = self.history_instantiations.clone();
        for c_type in &history_instantiations {
            let suffix = UserTypeInstantiation::suffix(c_type);
            let name = format!("History_{suffix}");

            self.write_line(&format!("typedef struct {{"));
            self.indent();
            self.write_line(&format!("{c_type}* states;"));
            self.write_line("DeloEventId* event_ids;");
            self.write_line("size_t length;");
            self.write_line("size_t capacity;");
            self.write_line("size_t cursor;");
            self.write_line("DeloEventId* log_events;");
            self.write_line(&format!("{c_type}* log_values;"));
            self.write_line("size_t log_len;");
            self.write_line("size_t log_cap;");
            self.un_indent();
            self.write_line(&format!("}} {name};"));
            self.write_line("");

            self.write_line(&format!("static void history_reset_cursor_{suffix}(void* p) {{ (({name}*)p)->cursor = 0; }}"));
            self.write_line(&format!("static void history_truncate_{suffix}(void* p) {{ (({name}*)p)->length = (({name}*)p)->cursor; }}"));
            self.write_line("");

            self.write_line(&format!("static {name}* history_alloc_{suffix}(void) {{"));
            self.indent();
            self.write_line(&format!("{name}* h = ({name}*)malloc(sizeof({name}));"));
            self.write_line("h->capacity = 4;");
            self.write_line(&format!("h->states = ({c_type}*)malloc(sizeof({c_type}) * h->capacity);"));
            self.write_line("h->event_ids = (DeloEventId*)malloc(sizeof(DeloEventId) * h->capacity);");
            self.write_line("h->length = 0;");
            self.write_line("h->cursor = 0;");
            self.write_line("h->log_cap = 0;");
            self.write_line("h->log_len = 0;");
            self.write_line("h->log_events = NULL;");
            self.write_line("h->log_values = NULL;");
            self.write_line(&format!("delo_register_history((void*)h, history_reset_cursor_{suffix}, history_truncate_{suffix});"));
            self.write_line("return h;");
            self.un_indent();
            self.write_line("}");
            self.write_line("");

            self.write_line(&format!("static bool history_check_log_{suffix}({name}* h, DeloEventId current, {c_type}* out) {{"));
            self.indent();
            self.write_line("for (size_t i = 0; i < h->log_len; i++) {");
            self.indent();
            self.write_line("if (delo_event_id_eq(h->log_events[i], current)) { *out = h->log_values[i]; return true; }");
            self.un_indent();
            self.write_line("}");
            self.write_line("return false;");
            self.un_indent();
            self.write_line("}");
            self.write_line("");

            self.write_line(&format!("static void history_log_{suffix}({name}* h, DeloEventId target, {c_type} val) {{"));
            self.indent();
            self.write_line("for (size_t i = 0; i < h->log_len; i++) {");
            self.indent();
            self.write_line("if (delo_event_id_eq(h->log_events[i], target)) { h->log_values[i] = val; return; }");
            self.un_indent();
            self.write_line("}");
            self.write_line("if (h->log_len >= h->log_cap) {");
            self.indent();
            self.write_line("h->log_cap = h->log_cap == 0 ? 4 : h->log_cap * 2;");
            self.write_line("h->log_events = (DeloEventId*)realloc(h->log_events, sizeof(DeloEventId) * h->log_cap);");
            self.write_line(&format!("h->log_values = ({c_type}*)realloc(h->log_values, sizeof({c_type}) * h->log_cap);"));
            self.un_indent();
            self.write_line("}");
            self.write_line("h->log_events[h->log_len] = target;");
            self.write_line("h->log_values[h->log_len] = val;");
            self.write_line("h->log_len++;");
            self.un_indent();
            self.write_line("}");
            self.write_line("");

            self.write_line(&format!("static void history_assign_{suffix}({name}* h, {c_type} computed_val) {{"));
            self.indent();
            self.write_line("delo_increment_local_event();");
            self.write_line("DeloEventId evt = delo_capture_event_id();");
            self.write_line(&format!("{c_type} val = computed_val;"));
            self.write_line(&format!("{c_type} forced;"));
            self.write_line(&format!("if (delo_replay_mode && history_check_log_{suffix}(h, evt, &forced)) val = forced;"));
            self.write_line("if (h->cursor < h->length) {");
            self.indent();
            self.write_line("h->states[h->cursor] = val;");
            self.write_line("h->event_ids[h->cursor] = evt;");
            self.un_indent();
            self.write_line("} else {");
            self.indent();
            self.write_line("if (h->length >= h->capacity) {");
            self.indent();
            self.write_line("h->capacity *= 2;");
            self.write_line(&format!("h->states = ({c_type}*)realloc(h->states, sizeof({c_type}) * h->capacity);"));
            self.write_line("h->event_ids = (DeloEventId*)realloc(h->event_ids, sizeof(DeloEventId) * h->capacity);");
            self.un_indent();
            self.write_line("}");
            self.write_line("h->states[h->length] = val;");
            self.write_line("h->event_ids[h->length] = evt;");
            self.write_line("h->length++;");
            self.un_indent();
            self.write_line("}");
            self.write_line("h->cursor++;");
            self.write_line("if (delo_replay_mode && delo_at_resume_event(evt)) { delo_replay_mode = false; delo_truncate_all(); }");
            self.un_indent();
            self.write_line("}");
            self.write_line("");

            self.write_line(&format!("static {c_type} history_current_{suffix}({name}* h) {{ return h->states[h->cursor - 1]; }}"));
            self.write_line("");

            self.write_line(&format!("static {c_type} history_get_{suffix}({name}* h, size_t index) {{"));
            self.indent();
            self.write_line("if (index >= h->length) { fprintf(stderr, \"time-travel index out of range\\n\"); exit(1); }");
            self.write_line("return h->states[index];");
            self.un_indent();
            self.write_line("}");
            self.write_line("");

            self.write_line(&format!("static void history_tt_write_{suffix}({name}* h, size_t index, {c_type} val) {{"));
            self.indent();
            self.write_line("delo_increment_local_event();");
            self.write_line("DeloEventId evt = delo_capture_event_id();");
            self.write_line("if (delo_replay_mode) {");
            self.indent();
            self.write_line("if (delo_at_resume_event(evt)) { delo_replay_mode = false; delo_truncate_all(); }");
            self.write_line("return;");
            self.un_indent();
            self.write_line("}");
            self.write_line("if (index >= h->length) { fprintf(stderr, \"time-travel write index out of range\\n\"); exit(1); }");
            self.write_line("DeloEventId target = h->event_ids[index];");
            self.write_line(&format!("history_log_{suffix}(h, target, val);"));
            self.write_line("delo_trigger_replay(evt);");
            self.un_indent();
            self.write_line("}");
            self.write_line("");
        }
    }

    fn emit_tracked_var_globals(&mut self, stmts: &[Stmt]) -> Result<(), CodegenError> {
        let mut emitted: BTreeSet<String> = BTreeSet::new();
        self.collect_tracked_var_decls(stmts, &mut emitted)?;
        Ok(())
    }

    fn collect_tracked_var_decls(&mut self, stmts: &[Stmt], emitted: &mut BTreeSet<String>) -> Result<(), CodegenError> {
        for stmt in stmts {
            self.collect_tracked_var_decls_in_stmt(stmt, emitted)?;
        }
        Ok(())
    }

    fn collect_tracked_var_decls_in_stmt(&mut self, stmt: &Stmt, emitted: &mut BTreeSet<String>) -> Result<(), CodegenError> {
        match stmt {
            Stmt::Variable { binding, type_annotation, initializer } => {
                if let VariableBinding::Identifier(token) = binding {
                    if self.types.tracked_vars.contains(&token.lexeme) && !emitted.contains(&token.lexeme) {
                        let delo_type = if let Some(annotation) = type_annotation {
                            annotation.clone()
                        } else if let Some(init) = initializer {
                            self.types.expr_types.get(&(init as *const Expr)).cloned().ok_or(CodegenError::MissingType {
                                line: token.line,
                                column: token.column,
                                identifier: token.lexeme.clone(),
                            })?
                        } else {
                            return Err(CodegenError::MissingType {
                                line: token.line,
                                column: token.column,
                                identifier: token.lexeme.clone(),
                            });
                        };
                        let c_type = self.map_type(&delo_type);
                        let suffix = UserTypeInstantiation::suffix(&c_type);
                        self.history_instantiations.insert(c_type);
                        let mangled = mangle(&token.lexeme);
                        self.write_line(&format!("static History_{suffix}* {mangled} = NULL;"));
                        emitted.insert(token.lexeme.clone());
                    }
                }
            }
            Stmt::If { then_branch, else_branch, .. } => {
                self.collect_tracked_var_decls_in_stmt(then_branch, emitted)?;
                if let Some(else_stmt) = else_branch {
                    self.collect_tracked_var_decls_in_stmt(else_stmt, emitted)?;
                }
            }
            Stmt::While { body, .. } => {
                self.collect_tracked_var_decls_in_stmt(body, emitted)?;
            }
            Stmt::ForIn { body, .. } => {
                self.collect_tracked_var_decls_in_stmt(body, emitted)?;
            }
            Stmt::Block(stmts) => {
                self.collect_tracked_var_decls(stmts, emitted)?;
            }
            Stmt::Function(function) => {
                self.collect_tracked_var_decls(&function.body, emitted)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn write_tuple_helper_declarations(&mut self) {
        let tuple_instantiations = self.tuple_instantiations.clone();
        for element_c_types in &tuple_instantiations {
            let name = tuple_c_type_name(element_c_types);
            let suffix = UserTypeInstantiation::suffix(&name);
            self.write_line(&format!("bool tuple_eq_{suffix}({name} a, {name} b);"));
            self.write_line(&format!("void format_{name}({name} v, FILE* stream);"));
        }
        if !tuple_instantiations.is_empty() {
            self.write_line("");
        }
    }

    fn write_tuple_helpers(&mut self) {
        let tuple_instantiations = self.tuple_instantiations.clone();

        for element_c_types in &tuple_instantiations {
            let name = tuple_c_type_name(element_c_types);
            let suffix = UserTypeInstantiation::suffix(&name);

            self.write_line(&format!("bool tuple_eq_{suffix}({name} a, {name} b) {{"));
            self.indent();
            for (i, element_c) in element_c_types.iter().enumerate() {
                let cmp = match element_c.as_str() {
                    "const char*" => format!("if (strcmp(a._{i}, b._{i}) != 0) return false;"),
                    "int" | "double" | "bool" => format!("if (a._{i} != b._{i}) return false;"),
                    other => {
                        let other_suffix = UserTypeInstantiation::suffix(other);
                        if other.starts_with("Tuple_") {
                            format!("if (!tuple_eq_{other_suffix}(a._{i}, b._{i})) return false;")
                        } else {
                            format!("if (memcmp(&a._{i}, &b._{i}, sizeof(a._{i})) != 0) return false;")
                        }
                    }
                };
                self.write_line(&cmp);
            }
            self.write_line("return true;");
            self.un_indent();
            self.write_line("}");
            self.write_line("");

            self.write_line(&format!("void format_{name}({name} v, FILE* stream) {{"));
            self.indent();
            self.write_line("fprintf(stream, \"(\");");
            for (i, element_c) in element_c_types.iter().enumerate() {
                if i > 0 {
                    self.write_line("fprintf(stream, \", \");");
                }
                let helper = self.format_helper_name(element_c);
                self.write_line(&format!("{helper}(v._{i}, stream);"));
            }
            if element_c_types.len() == 1 {
                self.write_line("fprintf(stream, \",\");");
            }
            self.write_line("fprintf(stream, \")\");");
            self.un_indent();
            self.write_line("}");
            self.write_line("");
        }
    }

    fn write_user_type_instantiations(&mut self) {
        let user_type_instantiations = self.user_type_instantiations.clone();

        for instantiation in user_type_instantiations {
            let c_identifier = instantiation.c_identifier();
            let identifier = &instantiation.identifier;

            if identifier == "Optional" {
                let inner_c_type = &instantiation.argument_c_types[0];
                let tag_name = format!("{c_identifier}_Tag");

                self.write_line(&format!("typedef enum {tag_name} {{"));
                self.indent();
                self.write_line(&format!("{}_{},", c_identifier, mangle("Some")));
                self.write_line(&format!("{}_{},", c_identifier, mangle("None")));
                self.un_indent();
                self.write_line(&format!("}} {tag_name};"));
                self.write_line("");

                self.write_line(&format!("typedef struct {c_identifier} {{"));
                self.indent();
                self.write_line(&format!("{tag_name} tag;"));
                self.write_line("union {");
                self.indent();
                self.write_line("struct {");
                self.indent();
                self.write_line(&format!("{inner_c_type} value;"));
                self.un_indent();
                self.write_line(&format!("}} {};", mangle("Some")));
                self.un_indent();
                self.write_line("} data;");
                self.un_indent();
                self.write_line(&format!("}} {c_identifier};"));
                self.write_line("");

                continue;
            }

            if let Some(Stmt::Struct { type_parameters, fields, .. }) = self.ast_structs.get(identifier) {
                let type_parameters = type_parameters.clone();
                let fields = fields.clone();
                self.write_struct_instantiation(&c_identifier, &type_parameters, &fields, &instantiation);
                continue;
            }

            if let Some(Stmt::Enum { type_parameters, variants, .. }) = self.ast_enums.get(identifier) {
                let type_parameters = type_parameters.clone();
                let variants = variants.clone();
                self.write_enum_instantiation(&c_identifier, &type_parameters, &variants, &instantiation);
                continue;
            }
        }
    }

    fn write_struct_instantiation(&mut self, c_identifier: &str, type_parameters: &Vec<Token>, fields: &Vec<(Token, TypeExpr)>, instantiation: &UserTypeInstantiation) {
            let bindings: Vec<(String, String)> = type_parameters
                .iter()
                .zip(instantiation.argument_c_types.iter())
                .map(|(parameter_token, c_type)| (parameter_token.lexeme.clone(), c_type.clone()))
                .collect();

            self.write_line(&format!("typedef struct {c_identifier} {{"));
            self.indent();
            for (field_identifier, field_type) in fields {
                let field_c_type = self.map_type_with_parameters(&field_type, &bindings);
                self.write_line(&format!("{} {};", field_c_type, mangle(&field_identifier.lexeme)));
            }
            self.un_indent();
            self.write_line(&format!("}} {c_identifier};"));
            self.write_line("");
    }

    fn write_enum_instantiation(&mut self, c_identifier: &str, type_parameters: &Vec<Token>, variants: &Vec<EnumVariant>, instantiation: &UserTypeInstantiation) {
        let bindings: Vec<(String, String)> = type_parameters
            .iter()
            .zip(instantiation.argument_c_types.iter())
            .map(|(parameter, c_type)| (parameter.lexeme.clone(), c_type.clone()))
            .collect();

        let tag_name = format!("{c_identifier}_Tag");
        self.write_line(&format!("typedef enum {tag_name} {{"));
        self.indent();
        for variant in variants {
            let tag_const = format!("{}_{}", c_identifier, mangle(&variant.identifier.lexeme));
            self.write_line(&format!("{tag_const},"));
        }
        self.un_indent();
        self.write_line(&format!("}} {tag_name};"));
        self.write_line("");

        self.write_line(&format!("typedef struct {c_identifier} {{"));
        self.indent();
        self.write_line(&format!("{tag_name} tag;"));

        let has_payloads = variants.iter().any(|v| !v.payload_types.is_empty());
        if has_payloads {
            self.write_line("union {");
            self.indent();

            for variant in variants {
                let variant_name = mangle(&variant.identifier.lexeme);
                if variant.payload_types.is_empty() {
                    continue;
                }

                self.write_line(&format!("struct {{"));
                self.indent();
                for (index, payload_type) in variant.payload_types.iter().enumerate() {
                    let field_c_type = self.map_type_with_parameters(payload_type, &bindings);
                    let field_name = if variant.payload_types.len() == 1 {
                        "value".to_string()
                    } else {
                        format!("field{index}")
                    };
                    self.write_line(&format!("{field_c_type} {field_name};"));
                }
                self.un_indent();
                self.write_line(&format!("}} {variant_name};"));
            }

            self.un_indent();
            self.write_line("} data;");
        }

        self.un_indent();
        self.write_line(&format!("}} {c_identifier};"));
        self.write_line("");
    }

    fn write_format_helpers(&mut self) {
        let array_instantiations = self.array_instantiations.clone();
        let map_instantiations = self.map_instantiations.clone();
        let range_instantiations = self.range_instantiations.clone();
        let user_type_instantiations = self.user_type_instantiations.clone();

        let non_generic_structs: Vec<String> = self.ast_structs.iter()
            .filter_map(|(name, stmt)| {
                if let Stmt::Struct { type_parameters, .. } = stmt {
                    if type_parameters.is_empty() { Some(name.clone()) } else { None }
                } else { None }
            })
            .collect();
        let non_generic_enums: Vec<String> = self.ast_enums.iter()
            .filter_map(|(name, stmt)| {
                if let Stmt::Enum { type_parameters, .. } = stmt {
                    if type_parameters.is_empty() { Some(name.clone()) } else { None }
                } else { None }
            })
            .collect();

        for elem in &array_instantiations {
            let suffix = UserTypeInstantiation::suffix(elem);
            self.write_raw(&format!("void format_Array_{suffix}(Array_{suffix} arr, FILE* stream);\n"));
        }
        for (k, v) in &map_instantiations {
            let k_suffix = UserTypeInstantiation::suffix(k);
            let v_suffix = UserTypeInstantiation::suffix(v);
            self.write_raw(&format!("void format_Map_{k_suffix}_{v_suffix}(Map_{k_suffix}_{v_suffix} m, FILE* stream);\n"));
        }
        for elem in &range_instantiations {
            let suffix = UserTypeInstantiation::suffix(elem);
            self.write_raw(&format!("void format_Range_{suffix}(Range_{suffix} r, FILE* stream);\n"));
        }
        for inst in &user_type_instantiations {
            let c_id = inst.c_identifier();
            self.write_raw(&format!("void format_{c_id}({c_id} v, FILE* stream);\n"));
        }
        for name in non_generic_structs.iter().chain(non_generic_enums.iter()) {
            let c_id = mangle(name);
            self.write_raw(&format!("void format_{c_id}({c_id} v, FILE* stream);\n"));
        }
        self.write_raw("\n");

        for elem in &array_instantiations {
            let suffix = UserTypeInstantiation::suffix(elem);
            let elem_helper = self.format_helper_name(elem);
            self.write_raw(&format!(r#"
                void format_Array_{suffix}(Array_{suffix} arr, FILE* stream) {{
                    fprintf(stream, "[");
                    for (size_t i = 0; i < arr.length; ++i) {{
                        if (i > 0) fprintf(stream, ", ");
                        {elem_helper}(arr.data[i], stream);
                    }}
                    fprintf(stream, "]");
                }}

            "#));
        }

        for (k, v) in &map_instantiations {
            let k_suffix = UserTypeInstantiation::suffix(k);
            let v_suffix = UserTypeInstantiation::suffix(v);
            let suffix = format!("{k_suffix}_{v_suffix}");
            let k_helper = self.format_helper_name(k);
            let v_helper = self.format_helper_name(v);
            self.write_raw(&format!(r#"
                void format_Map_{suffix}(Map_{suffix} m, FILE* stream) {{
                    fprintf(stream, "{{");
                    bool first = true;
                    for (size_t i = 0; i < m.capacity; ++i) {{
                        if (!m.slots[i].occupied) continue;
                        if (!first) fprintf(stream, ", ");
                        {k_helper}(m.slots[i].key, stream);
                        fprintf(stream, ": ");
                        {v_helper}(m.slots[i].value, stream);
                        first = false;
                    }}
                    fprintf(stream, "}}");
                }}

            "#));
        }

        for elem in &range_instantiations {
            let suffix = UserTypeInstantiation::suffix(elem);
            let elem_helper = self.format_helper_name(elem);
            self.write_raw(&format!(r#"
                void format_Range_{suffix}(Range_{suffix} r, FILE* stream) {{
                    {elem_helper}(r.start, stream);
                    fprintf(stream, r.inclusive ? "..=" : "..");
                    {elem_helper}(r.end, stream);
                }}

            "#));
        }

        for inst in &user_type_instantiations {
            self.write_user_type_format_helper(inst);
        }

        for name in &non_generic_structs {
            let inst = UserTypeInstantiation {
                identifier: name.clone(),
                argument_c_types: Vec::new(),
            };
            self.write_user_type_format_helper(&inst);
        }
        for name in &non_generic_enums {
            let inst = UserTypeInstantiation {
                identifier: name.clone(),
                argument_c_types: Vec::new(),
            };
            self.write_user_type_format_helper(&inst);
        }
    }

    fn write_user_type_format_helper(&mut self, instantiation: &UserTypeInstantiation) {
        let c_id = instantiation.c_identifier();
        let identifier = &instantiation.identifier;

        if identifier == "Optional" {
            let inner_c = &instantiation.argument_c_types[0];
            let inner_helper = self.format_helper_name(inner_c);
            let some_tag = format!("{c_id}_{}", mangle("Some"));
            let none_tag = format!("{c_id}_{}", mangle("None"));
            let some_member = mangle("Some");
            self.write_raw(&format!(r#"
                void format_{c_id}({c_id} v, FILE* stream) {{
                    if (v.tag == {some_tag}) {{
                        fprintf(stream, "Some(");
                        {inner_helper}(v.data.{some_member}.value, stream);
                        fprintf(stream, ")");
                    }} else if (v.tag == {none_tag}) {{
                        fprintf(stream, "None");
                    }}
                }}

            "#));
            return;
        }

        let bindings: Vec<(String, String)> = if let Some(Stmt::Struct { type_parameters, .. }) = self.ast_structs.get(identifier) {
            type_parameters.iter().zip(instantiation.argument_c_types.iter())
                .map(|(p, c)| (p.lexeme.clone(), c.clone()))
                .collect()
        } else if let Some(Stmt::Enum { type_parameters, .. }) = self.ast_enums.get(identifier) {
            type_parameters.iter().zip(instantiation.argument_c_types.iter())
                .map(|(p, c)| (p.lexeme.clone(), c.clone()))
                .collect()
        } else {
            return;
        };

        if let Some(Stmt::Struct { fields, .. }) = self.ast_structs.get(identifier).cloned() {
            let mut body = String::new();
            body.push_str(&format!("    fprintf(stream, \"{identifier} {{ \");\n"));
            for (i, (field_id, field_type)) in fields.iter().enumerate() {
                let field_c = self.map_type_with_parameters(field_type, &bindings);
                let field_helper = self.format_helper_name(&field_c);
                let field_name = mangle(&field_id.lexeme);
                if i > 0 {
                    body.push_str("    fprintf(stream, \", \");\n");
                }
                body.push_str(&format!("    fprintf(stream, \"{}: \");\n", field_id.lexeme));
                body.push_str(&format!("    {field_helper}(v.{field_name}, stream);\n"));
            }
            body.push_str("    fprintf(stream, \" }\");\n");

            self.write_raw(&format!(
                "void format_{c_id}({c_id} v, FILE* stream) {{\n{body}}}\n\n"
            ));
            return;
        }

        if let Some(Stmt::Enum { variants, .. }) = self.ast_enums.get(identifier).cloned() {
            let mut body = String::new();
            body.push_str("    switch (v.tag) {\n");
            for variant in &variants {
                let variant_tag = format!("{c_id}_{}", mangle(&variant.identifier.lexeme));
                let variant_member = mangle(&variant.identifier.lexeme);
                body.push_str(&format!("    case {variant_tag}:\n"));
                body.push_str(&format!("        fprintf(stream, \"{}\");\n", variant.identifier.lexeme));
                if !variant.payload_types.is_empty() {
                    body.push_str("        fprintf(stream, \"(\");\n");
                    if variant.payload_types.len() == 1 {
                        let payload_c = self.map_type_with_parameters(&variant.payload_types[0], &bindings);
                        let payload_helper = self.format_helper_name(&payload_c);
                        body.push_str(&format!("        {payload_helper}(v.data.{variant_member}.value, stream);\n"));
                    } else {
                        for (i, payload_type) in variant.payload_types.iter().enumerate() {
                            if i > 0 {
                                body.push_str("        fprintf(stream, \", \");\n");
                            }
                            let payload_c = self.map_type_with_parameters(payload_type, &bindings);
                            let payload_helper = self.format_helper_name(&payload_c);
                            body.push_str(&format!("        {payload_helper}(v.data.{variant_member}.field{i}, stream);\n"));
                        }
                    }
                    body.push_str("        fprintf(stream, \")\");\n");
                }
                body.push_str("        break;\n");
            }
            body.push_str("    }\n");

            self.write_raw(&format!(
                "void format_{c_id}({c_id} v, FILE* stream) {{\n{body}}}\n\n"
            ));
        }
    }

    fn write_function_instantiations(&mut self) -> Result<(), CodegenError> {
        let instantiations = mem::take(&mut self.function_instantiations);

        for instantiation in instantiations {
            let c_identifier = instantiation.c_identifier();
            let function = self.ast_functions.get(&instantiation.base_identifier).unwrap().clone();
            let argument_types = &instantiation.argument_c_types;
            self.write_function_instantiation(&c_identifier, &function, argument_types)?;
        }

        Ok(())
    }

    fn write_function_instantiation(&mut self, c_identifier: &str, function: &FunctionStmt, argument_c_types: &Vec<String>) -> Result<(), CodegenError> {
        let bindings: Vec<(String, String)> = function
            .type_parameters
            .iter()
            .zip(argument_c_types.iter())
            .map(|(parameter, argument_c_type)| (parameter.lexeme.clone(), argument_c_type.clone()))
            .collect();

        let return_c_type = if let Some(TypeExpr::Function { return_type, .. }) = self.types.function_types.get(&function.identifier.lexeme) {
            self.map_type_with_parameters(return_type, &bindings)
        } else {
            match &function.return_type {
                Some(return_type) => self.map_type_with_parameters(&return_type, &bindings),
                None => "void".to_string(),
            }
        };

        let mut parameters_code = Vec::new();
        for (parameter_identifier, parameter_type) in &function.parameters {
            let parameter_c_type = self.map_type_with_parameters(parameter_type, &bindings);
            parameters_code.push(format!("{} {}", parameter_c_type, mangle(&parameter_identifier.lexeme)));
        }
        let parameters_code = if parameters_code.is_empty() {
            "void".to_string()
        } else {
            parameters_code.join(", ")
        };

        self.write_line(&format!("{return_c_type} {c_identifier}({parameters_code}) {{"));
        self.indent();

        for (index, stmt) in function.body.iter().enumerate() {
            let is_last = index == function.body.len() - 1;

            if return_c_type != "void" && is_last {
                if let Stmt::Expression(last_expr) = stmt {
                    let last_code = self.generate_expr(last_expr)?;
                    for line in last_code.pre_expr_stmts {
                        self.write_line(&line);
                    }
                    self.write_line(&format!("return {};", last_code.expr));
                    continue;
                }
            }

            let mut lines = Vec::new();
            self.generate_stmt_to(&mut lines, stmt)?;
            for line in lines {
                self.output.push_str(&line);
                self.output.push('\n');
            }
        }

        self.un_indent();
        self.write_line("}");
        self.write_line("");

        Ok(())
    }

    fn generate_main(&mut self, stmts: &[Stmt]) -> Result<(), CodegenError> {
        self.write_line("int main(void) {");
        self.indent();
        self.write_line("if (!delo_init_done) {");
        self.indent();
        self.write_line("delo_init_siphash_key();");
        self.write_line("delo_init_done = true;");
        self.un_indent();
        self.write_line("}");
        self.write_line("setjmp(delo_restart_env);");
        self.write_line("if (delo_stack_depth == 0) {");
        self.indent();
        self.write_line("delo_call_push();");
        self.un_indent();
        self.write_line("}");

        for stmt in stmts {
            let mut lines = Vec::new();
            self.generate_stmt_to(&mut lines, stmt)?;
            for line in lines {
                self.output.push_str(&line);
                self.output.push('\n');
            }
        }

        self.write_line("return 0;");
        self.un_indent();
        self.write_line("}");
        self.write_line("");

        Ok(())
    }

    fn generate_stmt_to(&mut self, out: &mut Vec<String>, stmt: &Stmt) -> Result<(), CodegenError> {
        match stmt {
            Stmt::Variable { binding, type_annotation, initializer } => {
                let first_token = binding.first_token();
                let binding_label = match binding {
                    VariableBinding::Identifier(token) => token.lexeme.clone(),
                    VariableBinding::Tuple { .. } => "(tuple)".to_string(),
                };

                let delo_type = if let Some(annotation) = type_annotation.as_ref() {
                    annotation.clone()
                } else if let Some(initializer_expr) = initializer {
                    if let Some(initializer_type) = self.types.expr_types.get(&(initializer_expr as *const Expr)) {
                        initializer_type.clone()
                    } else {
                        return Err(CodegenError::MissingType {
                            line: first_token.line,
                            column: first_token.column,
                            identifier: binding_label,
                        });
                    }
                } else {
                    return Err(CodegenError::MissingType {
                        line: first_token.line,
                        column: first_token.column,
                        identifier: binding_label,
                    });
                };
                let c_type = self.map_type(&delo_type);

                let init_code = if let Some(expr) = initializer {
                    let expr_code = self.generate_expr(expr)?;
                    out.extend(expr_code.pre_expr_stmts);
                    expr_code.expr
                } else {
                    default_value_for_c_type(&c_type).to_string()
                };

                if let VariableBinding::Identifier(token) = binding {
                    if self.types.tracked_vars.contains(&token.lexeme) {
                        let suffix = UserTypeInstantiation::suffix(&c_type);
                        self.history_instantiations.insert(c_type.clone());
                        let mangled = mangle(&token.lexeme);
                        self.write_indented_to(out, self.indent_level, format!("if (!{mangled}) {mangled} = history_alloc_{suffix}();"));
                        self.write_indented_to(out, self.indent_level, format!("history_assign_{suffix}({mangled}, {init_code});"));
                        return Ok(());
                    }
                }

                self.emit_variable_binding(out, binding, &delo_type, &c_type, &init_code)?;
            }
            Stmt::Enum { .. } => {}
            Stmt::Struct { .. } => {}
            Stmt::If { condition, then_branch, else_branch, .. } => {
                let condition_code = self.generate_expr(condition)?;
                for line in condition_code.pre_expr_stmts {
                    self.write_indented_to(out, self.indent_level, line);
                }
                self.write_indented_to(out, self.indent_level, format!("if ({}) {{", condition_code.expr));
                self.indent();
                self.generate_stmt_to(out, then_branch)?;
                self.un_indent();
                self.write_indented_to(out, self.indent_level, "}".to_string());

                if let Some(else_stmt) = else_branch {
                    self.write_indented_to(out, self.indent_level, "else {".to_string());
                    self.indent();
                    self.generate_stmt_to(out, else_stmt)?;
                    self.un_indent();
                    self.write_indented_to(out, self.indent_level, "}".to_string());
                }
            }
            Stmt::While { condition, body, .. } => {
                let condition_code = self.generate_expr(condition)?;
                for line in condition_code.pre_expr_stmts {
                    self.write_indented_to(out, self.indent_level, line);
                }
                self.write_indented_to(out, self.indent_level, format!("while ({}) {{", condition_code.expr));
                self.indent();
                self.generate_stmt_to(out, body)?;
                self.un_indent();
                self.write_indented_to(out, self.indent_level, "}".to_string());
            }
            Stmt::ForIn { binding, iterable, body } => {
                let iterable_code = self.generate_expr(iterable)?;
                for line in iterable_code.pre_expr_stmts {
                    self.write_indented_to(out, self.indent_level, line);
                }
                let iterable_expr = iterable_code.expr;

                let iterable_type = self.types.expr_types[&(iterable as *const Expr)].clone();
                let first_token = binding.first_token();

                enum ForInKind { Range, Array, Map }
                let (kind, element_delo_type, key_type_opt, value_type_opt) = match &iterable_type {
                    TypeExpr::Named { identifier, type_arguments, .. }
                        if (identifier.lexeme == "Range" || identifier.lexeme == "InclusiveRange") && type_arguments.len() == 1 =>
                    {
                        (ForInKind::Range, type_arguments[0].clone(), None, None)
                    }
                    TypeExpr::Named { identifier, type_arguments, .. }
                        if identifier.lexeme == "Array" && type_arguments.len() == 1 =>
                    {
                        (ForInKind::Array, type_arguments[0].clone(), None, None)
                    }
                    TypeExpr::Named { identifier, type_arguments, .. }
                        if identifier.lexeme == "Map" && type_arguments.len() == 2 =>
                    {
                        let entry_type = TypeExpr::Tuple {
                            element_types: vec![type_arguments[0].clone(), type_arguments[1].clone()],
                        };
                        (ForInKind::Map, entry_type, Some(type_arguments[0].clone()), Some(type_arguments[1].clone()))
                    }
                    _ => {
                        return Err(CodegenError::UnexpectedTypeInCodegen {
                            line: first_token.line,
                            column: first_token.column,
                            expected: "for-in iterable (Range, InclusiveRange, Array, or Map)",
                            found_type: iterable_type.clone(),
                        });
                    }
                };

                let element_c_type = self.map_type(&element_delo_type);

                match kind {
                    ForInKind::Range => {
                        let range_c_type = self.map_type(&iterable_type);
                        let range_temp = self.new_temp_id("range_");

                        self.write_indented_to(out, self.indent_level, format!("{range_c_type} {range_temp} = {iterable_expr};"));

                        let i_temp = self.new_temp_id("range_i_");
                        self.write_indented_to(
                            out,
                            self.indent_level,
                            format!(
                                "for ({t} {i} = {r}.start; ({r}.inclusive ? {i} <= {r}.end : {i} < {r}.end); ++{i}) {{",
                                t = element_c_type,
                                i = i_temp,
                                r = range_temp,
                            )
                        );

                        self.indent();
                        self.emit_variable_binding(out, binding, &element_delo_type, &element_c_type, &i_temp)?;
                        self.generate_stmt_to(out, body)?;
                        self.un_indent();
                        self.write_indented_to(out, self.indent_level, "}".to_string());
                    }
                    ForInKind::Array => {
                        let array_c_type = self.map_type(&iterable_type);
                        let array_temp = self.new_temp_id("array_");
                        let index_temp = self.new_temp_id("array_index_");

                        self.write_indented_to(out, self.indent_level, format!("{array_c_type} {array_temp} = {iterable_expr};"));

                        self.write_indented_to(
                            out,
                            self.indent_level,
                            format!("for (size_t {index_temp} = 0; {index_temp} < {array_temp}.length; ++{index_temp}) {{"),
                        );

                        self.indent();
                        let init = format!("{array_temp}.data[{index_temp}]");
                        self.emit_variable_binding(out, binding, &element_delo_type, &element_c_type, &init)?;
                        self.generate_stmt_to(out, body)?;
                        self.un_indent();
                        self.write_indented_to(out, self.indent_level, "}".to_string());
                    }
                    ForInKind::Map => {
                        let map_c_type = self.map_type(&iterable_type);
                        let map_temp = self.new_temp_id("map_");
                        let slot_index_temp = self.new_temp_id("map_slot_");

                        let key_c_type = self.map_type(&key_type_opt.unwrap());
                        let value_c_type = self.map_type(&value_type_opt.unwrap());
                        let key_suffix = UserTypeInstantiation::suffix(&key_c_type);
                        let value_suffix = UserTypeInstantiation::suffix(&value_c_type);
                        let suffix = format!("{key_suffix}_{value_suffix}");

                        self.write_indented_to(out, self.indent_level, format!("{map_c_type} {map_temp} = {iterable_expr};"));

                        self.write_indented_to(
                            out,
                            self.indent_level,
                            format!("for (size_t {slot_index_temp} = 0; {slot_index_temp} < {map_temp}.capacity; ++{slot_index_temp}) {{"),
                        );

                        self.indent();
                        self.write_indented_to(out, self.indent_level, format!("if (!{map_temp}.slots[{slot_index_temp}].occupied) continue;"));
                        let init = format!(
                            "({tuple_c}){{ ._0 = {m}.slots[{i}].key, ._1 = {m}.slots[{i}].value }}",
                            tuple_c = element_c_type,
                            m = map_temp,
                            i = slot_index_temp,
                        );
                        let _ = suffix;
                        self.emit_variable_binding(out, binding, &element_delo_type, &element_c_type, &init)?;
                        self.generate_stmt_to(out, body)?;
                        self.un_indent();
                        self.write_indented_to(out, self.indent_level, "}".to_string());
                    }
                }
            }
            Stmt::Function(_) => {}
            Stmt::Block(stmts) => {
                for stmt in stmts {
                    self.generate_stmt_to(out,stmt)?;
                }
            }
            Stmt::Expression(expr) => {
                let code = self.generate_expr(expr)?;
                out.extend(code.pre_expr_stmts);
                self.write_indented_to(out, self.indent_level, format!("{};", code.expr));
            }
            Stmt::Break(_) => {
                self.write_indented_to(out, self.indent_level, "break;".to_string());
            }
            Stmt::Continue(_) => {
                self.write_indented_to(out, self.indent_level, "continue;".to_string());
            }
        }

        Ok(())
    }

    fn generate_expr(&mut self, expr: &Expr) -> Result<ExprCode, CodegenError> {
        match expr {
            Expr::Literal(token) => {
                Ok(ExprCode {
                    pre_expr_stmts: Vec::new(),
                    expr: token.lexeme.clone()
                })
            }
            Expr::ArrayLiteral { elements, .. } => {
                let result_type = self.types.expr_types[&(expr as *const Expr)].clone();
                self.emit_array_literal(elements, &result_type)
            }
            Expr::MapLiteral { elements, .. } => {
                let result_type = self.types.expr_types[&(expr as *const Expr)].clone();
                let keys: Vec<&Expr> = elements.iter().map(|(k, _)| k).collect();
                let values: Vec<&Expr> = elements.iter().map(|(_, v)| v).collect();
                self.emit_map_literal(&keys, &values, &result_type)
            }
            Expr::TupleLiteral { elements, .. } => {
                let result_type = self.types.expr_types[&(expr as *const Expr)].clone();
                self.emit_tuple_literal(elements, &result_type)
            }
            Expr::RangeLiteral { start, end, is_inclusive, .. } => {
                let result_type = self.types.expr_types[&(expr as *const Expr)].clone();
                self.emit_range_literal(start, end, *is_inclusive, &result_type)
            }
            Expr::Variable(identifier) => {
                let var_type = self.types.expr_types.get(&(expr as *const Expr));

                let expr_string = if let Some(var_type) = var_type {
                    match var_type {
                        TypeExpr::Named { identifier: type_identifier, type_arguments, .. }
                            if type_identifier.lexeme == "Optional"
                                && type_arguments.len() == 1
                                && identifier.lexeme == "None" =>
                        {
                            let enum_c_type = self.map_type(var_type);
                            let tag_const = format!("{enum_c_type}_{}", mangle("None"));
                            format!("({enum_c_type}){{ .tag = {tag_const} }}")
                        }

                        TypeExpr::Named { identifier: type_identifier, .. } if type_identifier.lexeme == "Bool" => {
                            match identifier.lexeme.as_str() {
                                "True" => "true".to_string(),
                                "False" => "false".to_string(),
                                _ if self.types.tracked_vars.contains(&identifier.lexeme) => {
                                    let c_type = self.map_type(var_type);
                                    let suffix = UserTypeInstantiation::suffix(&c_type);
                                    format!("history_current_{suffix}({})", mangle(&identifier.lexeme))
                                }
                                _ => mangle(&identifier.lexeme),
                            }
                        }

                        _ if self.types.tracked_vars.contains(&identifier.lexeme) => {
                            let c_type = self.map_type(var_type);
                            let suffix = UserTypeInstantiation::suffix(&c_type);
                            format!("history_current_{suffix}({})", mangle(&identifier.lexeme))
                        }

                        _ => mangle(&identifier.lexeme),
                    }
                } else {
                    mangle(&identifier.lexeme)
                };

                Ok(ExprCode {
                    pre_expr_stmts: Vec::new(),
                    expr: expr_string
                })
            }
            Expr::StructInstantiation { fields, .. } => {
                let mut pre_expr_stmts = Vec::new();

                let struct_type = &self.types.expr_types[&(expr as *const Expr)];
                let struct_c_type = self.map_type(struct_type);

                let mut field_strings = Vec::new();
                for (field_name, field_expr) in fields {
                    let expr_code = self.generate_expr(field_expr)?;
                    pre_expr_stmts.extend(expr_code.pre_expr_stmts);
                    field_strings.push(format!(".{} = {}", mangle(&field_name.lexeme), expr_code.expr));
                }

                let expr_string = format!("({}){{{}}}", struct_c_type, field_strings.join(", "));

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: expr_string
                })
            }
            Expr::Assign { identifier, value } => {
                let mut pre_expr_stmts = Vec::new();
                let expr_code = self.generate_expr(value)?;
                pre_expr_stmts.extend(expr_code.pre_expr_stmts);

                let expr_string = if self.types.tracked_vars.contains(&identifier.lexeme) {
                    let var_type = self.types.expr_types.get(&(expr as *const Expr)).cloned()
                        .or_else(|| self.types.expr_types.get(&(value.as_ref() as *const Expr)).cloned());
                    let c_type = match var_type {
                        Some(t) => self.map_type(&t),
                        None => return Err(CodegenError::MissingType {
                            line: identifier.line,
                            column: identifier.column,
                            identifier: identifier.lexeme.clone(),
                        }),
                    };
                    let suffix = UserTypeInstantiation::suffix(&c_type);
                    self.history_instantiations.insert(c_type.clone());
                    let mangled = mangle(&identifier.lexeme);
                    format!("(history_assign_{suffix}({mangled}, {}), history_current_{suffix}({mangled}))", expr_code.expr)
                } else {
                    format!("({} = {})", mangle(&identifier.lexeme), expr_code.expr)
                };

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: expr_string
                })
            }
            Expr::AssignIndex { target, index, value, .. } => {
                let mut pre_expr_stmts = Vec::new();

                let target_type = self.types.expr_types[&(target.as_ref() as *const Expr)].clone();

                let target_code = self.generate_expr(target)?;
                pre_expr_stmts.extend(target_code.pre_expr_stmts);
                let target_expr = target_code.expr;

                let index_code = self.generate_expr(index)?;
                pre_expr_stmts.extend(index_code.pre_expr_stmts);
                let index_expr = index_code.expr;

                let value_code = self.generate_expr(value)?;
                pre_expr_stmts.extend(value_code.pre_expr_stmts);
                let value_expr = value_code.expr;

                if let TypeExpr::Named { identifier, type_arguments, .. } = &target_type {
                    if identifier.lexeme == "Map" && type_arguments.len() == 2 {
                        let key_c_type = self.map_type(&type_arguments[0]);
                        let value_c_type = self.map_type(&type_arguments[1]);
                        let key_suffix = UserTypeInstantiation::suffix(&key_c_type);
                        let value_suffix = UserTypeInstantiation::suffix(&value_c_type);
                        let suffix = format!("{key_suffix}_{value_suffix}");
                        self.map_instantiations.insert((key_c_type, value_c_type.clone()));

                        let value_temp = self.new_temp_id("assign_index_value_");
                        self.write_indented_to(&mut pre_expr_stmts, self.indent_level, format!("{value_c_type} {value_temp} = {value_expr};"));
                        self.write_indented_to(&mut pre_expr_stmts, self.indent_level, format!("map_set_{suffix}(&{target_expr}, {index_expr}, {value_temp});"));
                        return Ok(ExprCode { pre_expr_stmts, expr: value_temp });
                    }
                }

                let expr_string = format!("({target_expr}.data[{index_expr}] = {value_expr})");

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: expr_string
                })
            }
            Expr::AssignTimeTravelAbsolute { target, index, value, at_token } => {
                let target_name = match target.as_ref() {
                    Expr::Variable(t) => t.lexeme.clone(),
                    _ => return Err(CodegenError::UnexpectedTypeInCodegen {
                        line: at_token.line,
                        column: at_token.column,
                        expected: "variable as time-travel write target",
                        found_type: self.types.expr_types[&(target.as_ref() as *const Expr)].clone(),
                    }),
                };
                let var_type = self.types.expr_types[&(target.as_ref() as *const Expr)].clone();
                let c_type = self.map_type(&var_type);
                let suffix = UserTypeInstantiation::suffix(&c_type);
                self.history_instantiations.insert(c_type.clone());

                let mut pre_expr_stmts = Vec::new();
                let index_code = self.generate_expr(index)?;
                pre_expr_stmts.extend(index_code.pre_expr_stmts);
                let value_code = self.generate_expr(value)?;
                pre_expr_stmts.extend(value_code.pre_expr_stmts);
                let mangled = mangle(&target_name);
                let value_temp = self.new_temp_id("tt_val_");
                self.write_indented_to(&mut pre_expr_stmts, self.indent_level, format!("{c_type} {value_temp} = {};", value_code.expr));
                self.write_indented_to(&mut pre_expr_stmts, self.indent_level, format!("history_tt_write_{suffix}({mangled}, (size_t)({}), {value_temp});", index_code.expr));
                Ok(ExprCode { pre_expr_stmts, expr: value_temp })
            }
            Expr::AssignTimeTravelRelative { target, offset, value, at_token } => {
                let target_name = match target.as_ref() {
                    Expr::Variable(t) => t.lexeme.clone(),
                    _ => return Err(CodegenError::UnexpectedTypeInCodegen {
                        line: at_token.line,
                        column: at_token.column,
                        expected: "variable as time-travel write target",
                        found_type: self.types.expr_types[&(target.as_ref() as *const Expr)].clone(),
                    }),
                };
                let var_type = self.types.expr_types[&(target.as_ref() as *const Expr)].clone();
                let c_type = self.map_type(&var_type);
                let suffix = UserTypeInstantiation::suffix(&c_type);
                self.history_instantiations.insert(c_type.clone());

                let mut pre_expr_stmts = Vec::new();
                let offset_code = self.generate_expr(offset)?;
                pre_expr_stmts.extend(offset_code.pre_expr_stmts);
                let value_code = self.generate_expr(value)?;
                pre_expr_stmts.extend(value_code.pre_expr_stmts);
                let mangled = mangle(&target_name);
                let value_temp = self.new_temp_id("tt_val_");
                self.write_indented_to(&mut pre_expr_stmts, self.indent_level, format!("{c_type} {value_temp} = {};", value_code.expr));
                self.write_indented_to(&mut pre_expr_stmts, self.indent_level, format!("history_tt_write_{suffix}({mangled}, {mangled}->length - 1 - (size_t)({}), {value_temp});", offset_code.expr));
                Ok(ExprCode { pre_expr_stmts, expr: value_temp })
            }
            Expr::Grouping(expr) => {
                let mut pre_expr_stmts = Vec::new();
                let expr_code = self.generate_expr(expr)?;
                pre_expr_stmts.extend(expr_code.pre_expr_stmts);

                let expr_string = format!("({})", expr_code.expr); 

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: expr_string
                })
            }
            Expr::Unary { operator, right } => {
                let mut pre_expr_stmts = Vec::new();
                let expr_code = self.generate_expr(right)?;
                pre_expr_stmts.extend(expr_code.pre_expr_stmts);
                
                let expr_string = format!("({}{})", operator.lexeme, expr_code.expr);

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: expr_string
                })
            }
            Expr::Binary { left, operator, right } => {
                let mut pre_expr_stmts = Vec::new();

                let left_code = self.generate_expr(left)?;
                pre_expr_stmts.extend(left_code.pre_expr_stmts);
                let left_expr = left_code.expr;

                if operator.lexeme == "??" {
                    let right_code = self.generate_expr(right)?;

                    let opt_type = self.types.expr_types[&(left.as_ref() as *const Expr)].clone();
                    let optional_c_type = self.map_type(&opt_type);
                    let result_type = self.types.expr_types[&(expr as *const Expr)].clone();
                    let result_c_type = self.map_type(&result_type);

                    let temp = self.new_temp_id("nil_coalesce_");
                    let result_temp = self.new_temp_id("nil_coalesce_result_");
                    let some_member = mangle("Some");
                    let some_tag = format!("{optional_c_type}_{some_member}");

                    pre_expr_stmts.push(format!("{optional_c_type} {temp} = {left_expr};"));
                    pre_expr_stmts.push(format!("{result_c_type} {result_temp};"));
                    pre_expr_stmts.push(format!("if ({temp}.tag == {some_tag}) {{"));
                    pre_expr_stmts.push(format!("    {result_temp} = {temp}.data.{some_member}.value;"));
                    pre_expr_stmts.push("} else {".to_string());
                    for line in right_code.pre_expr_stmts {
                        pre_expr_stmts.push(format!("    {line}"));
                    }
                    pre_expr_stmts.push(format!("    {result_temp} = {};", right_code.expr));
                    pre_expr_stmts.push("}".to_string());

                    return Ok(ExprCode { pre_expr_stmts, expr: result_temp });
                }

                let right_code = self.generate_expr(right)?;
                pre_expr_stmts.extend(right_code.pre_expr_stmts);
                let right_expr = right_code.expr;

                if operator.lexeme == "+" {
                    if let Some(result_type) = self.types.expr_types.get(&(expr as *const Expr)).cloned() {
                        if let TypeExpr::Named { identifier, type_arguments, .. } = &result_type {
                            if identifier.lexeme == "String" {
                                let expr_string = format!("string_concat({left_expr}, {right_expr})");
                                return Ok(ExprCode { pre_expr_stmts, expr: expr_string });
                            }
                            if identifier.lexeme == "Array" && type_arguments.len() == 1 {
                                let element_c_type = self.map_type(&type_arguments[0]);
                                self.array_instantiations.insert(element_c_type.clone());
                                let suffix = UserTypeInstantiation::suffix(&element_c_type);
                                let expr_string = format!("array_concat_{suffix}({left_expr}, {right_expr})");
                                return Ok(ExprCode { pre_expr_stmts, expr: expr_string });
                            }
                        }
                    }
                }

                if operator.lexeme == "*" {
                    if let Some(result_type) = self.types.expr_types.get(&(expr as *const Expr)).cloned() {
                        if let TypeExpr::Named { identifier, type_arguments, .. } = &result_type {
                            if identifier.lexeme == "String" {
                                let left_type = self.types.expr_types.get(&(left.as_ref() as *const Expr));
                                let left_is_string = matches!(left_type, Some(TypeExpr::Named { identifier, .. }) if identifier.lexeme == "String");
                                let expr_string = if left_is_string {
                                    format!("string_repeat({left_expr}, {right_expr})")
                                } else {
                                    format!("string_repeat({right_expr}, {left_expr})")
                                };
                                return Ok(ExprCode { pre_expr_stmts, expr: expr_string });
                            }
                            if identifier.lexeme == "Array" && type_arguments.len() == 1 {
                                let element_c_type = self.map_type(&type_arguments[0]);
                                self.array_instantiations.insert(element_c_type.clone());
                                let suffix = UserTypeInstantiation::suffix(&element_c_type);
                                let left_type = self.types.expr_types.get(&(left.as_ref() as *const Expr));
                                let left_is_array = matches!(left_type, Some(TypeExpr::Named { identifier, .. }) if identifier.lexeme == "Array");
                                let expr_string = if left_is_array {
                                    format!("array_repeat_{suffix}({left_expr}, {right_expr})")
                                } else {
                                    format!("array_repeat_{suffix}({right_expr}, {left_expr})")
                                };
                                return Ok(ExprCode { pre_expr_stmts, expr: expr_string });
                            }
                        }
                    }
                }

                if operator.lexeme == "^" {
                    if let Some(TypeExpr::Named { identifier, .. }) = self.types.expr_types.get(&(expr as *const Expr)) {
                        let helper = match identifier.lexeme.as_str() {
                            "Int" => "int_pow",
                            _ => "pow",
                        };
                        let expr_string = format!("{helper}({left_expr}, {right_expr})");

                        return Ok(ExprCode {
                            pre_expr_stmts,
                            expr: expr_string
                        });
                    }
                }

                if operator.lexeme == "==" || operator.lexeme == "!=" {
                    let left_type = self.types.expr_types.get(&(left.as_ref() as *const Expr)).cloned();
                    if let Some(TypeExpr::Tuple { element_types }) = left_type {
                        let element_c_types: Vec<String> = element_types.iter().map(|t| self.map_type(t)).collect();
                        let tuple_name = tuple_c_type_name(&element_c_types);
                        self.tuple_instantiations.insert(element_c_types);
                        let suffix = UserTypeInstantiation::suffix(&tuple_name);
                        let call = format!("tuple_eq_{suffix}({left_expr}, {right_expr})");
                        let expr_string = if operator.lexeme == "==" { call } else { format!("(!{call})") };
                        return Ok(ExprCode { pre_expr_stmts, expr: expr_string });
                    }
                }

                let expr_s = format!("({} {} {})", left_expr, operator.lexeme, right_expr);
                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: expr_s
                })
            }
            Expr::Logical { left, operator, right } => {
                let mut pre_expr_stmts = Vec::new();

                let left_code = self.generate_expr(left)?;
                pre_expr_stmts.extend(left_code.pre_expr_stmts);
                let left_expr = left_code.expr;

                let right_code = self.generate_expr(right)?;

                let temp = self.new_temp_id("short_circuit_");
                let result = self.new_temp_id("short_circuit_result_");

                let (eval_rhs_condition, short_circuit_value) = match operator.lexeme.as_str() {
                    "&&" => (temp.clone(), "false"),
                    "||" => (format!("!{temp}"), "true"),
                    _ => unreachable!(),
                };

                pre_expr_stmts.push(format!("bool {temp} = {left_expr};"));
                pre_expr_stmts.push(format!("bool {result};"));
                pre_expr_stmts.push(format!("if ({eval_rhs_condition}) {{"));
                for line in right_code.pre_expr_stmts {
                    pre_expr_stmts.push(format!("    {line}"));
                }
                pre_expr_stmts.push(format!("    {result} = {};", right_code.expr));
                pre_expr_stmts.push("} else {".to_string());
                pre_expr_stmts.push(format!("    {result} = {short_circuit_value};"));
                pre_expr_stmts.push("}".to_string());

                let expr_string = result;

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: expr_string
                })
            }
            Expr::Call { callee, arguments, .. } => {
                if let Some(kind) = self.types.builtin_calls.get(&(expr as *const Expr)).copied() {
                    return self.emit_builtin_call(kind, arguments, expr);
                }

                let mut pre_expr_stmts = Vec::new();

                let callee_code = self.generate_expr(callee)?;
                pre_expr_stmts.extend(callee_code.pre_expr_stmts);
                let callee_expr = callee_code.expr;

                if let Expr::Variable(ref identifier) = **callee {
                    let call_type = &self.types.expr_types.get(&(expr as *const Expr)).ok_or(CodegenError::MissingType {
                        line: identifier.line,
                        column: identifier.column,
                        identifier: identifier.lexeme.clone(),
                    })?;

                    if let TypeExpr::Named { enum_variants, .. } = call_type {
                        if let Some(variants) = enum_variants {
                            if let Some(variant) = variants.iter().find(|v| v.identifier.lexeme == identifier.lexeme) {
                                let mut argument_exprs = Vec::new();
                                for argument in arguments {
                                    let argument_code = self.generate_expr(argument)?;
                                    pre_expr_stmts.extend(argument_code.pre_expr_stmts);
                                    argument_exprs.push(argument_code.expr);
                                }

                                let enum_c_type = self.map_type(call_type);
                                let tag_const = format!("{}_{}", enum_c_type, mangle(&variant.identifier.lexeme));

                                let expr_string = if variant.payload_types.is_empty() {
                                    if !argument_exprs.is_empty() {
                                        return Err(CodegenError::InvalidEnumArgumentCount {
                                            line: variant.identifier.line,
                                            column: variant.identifier.column,
                                            variant_identifier: variant.identifier.lexeme.clone(),
                                            expected: 0,
                                            found: argument_exprs.len()
                                        });
                                    }
                                    format!("({enum_c_type}){{ .tag = {tag_const} }}")
                                } else {
                                    if argument_exprs.len() != variant.payload_types.len() {
                                        return Err(CodegenError::InvalidEnumArgumentCount {
                                            line: variant.identifier.line,
                                            column: variant.identifier.column,
                                            variant_identifier: variant.identifier.lexeme.clone(),
                                            expected: variant.payload_types.len(),
                                            found: argument_exprs.len(),
                                        });
                                    }
                                    let variant_member = mangle(&identifier.lexeme);
                                    let payload_init = if variant.payload_types.len() == 1 {
                                        format!(".value = {}", argument_exprs[0])
                                    } else {
                                        argument_exprs
                                            .iter()
                                            .enumerate()
                                            .map(|(index, expr)| format!(".field{index} = {expr}"))
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    };
                                    format!("({enum_c_type}){{ .tag = {tag_const}, .data.{variant_member} = {{ {payload_init} }} }}")
                                };

                                return Ok(ExprCode { 
                                    pre_expr_stmts, 
                                    expr: expr_string 
                                });
                            }
                        }
                    }
                }

                let mut argument_exprs = Vec::new();
                for argument in arguments {
                    let argument_code = self.generate_expr(argument)?;
                    pre_expr_stmts.extend(argument_code.pre_expr_stmts);
                    argument_exprs.push(argument_code.expr);
                }
                let arguments_code = argument_exprs.join(", ");

                let mut callee_identifier: Option<String> = None;
                if let Expr::Variable(ref ident) = **callee {
                    callee_identifier = Some(ident.lexeme.clone());
                }

                if let Some(ref identifier) = callee_identifier {
                    if self.ast_functions.contains_key(identifier) {
                        let argument_c_types: Vec<String> = self.types.call_type_arguments
                            .get(&(expr as *const Expr))
                            .cloned()
                            .unwrap_or_default()
                            .iter()
                            .map(|t| self.map_type(t))
                            .collect();

                        let instantiation = FunctionInstantiation {
                            base_identifier: identifier.clone(),
                            argument_c_types,
                        };

                        let instantiated_c_identifier = instantiation.c_identifier();
                        self.function_instantiations.insert(instantiation);

                        let return_type = self.types.expr_types.get(&(expr as *const Expr)).cloned();
                        let is_void = matches!(&return_type, Some(TypeExpr::Named { identifier, .. }) if identifier.lexeme == "Void");
                        pre_expr_stmts.push("delo_call_push();".to_string());
                        if is_void {
                            pre_expr_stmts.push(format!("{instantiated_c_identifier}({arguments_code});"));
                            pre_expr_stmts.push("delo_call_pop();".to_string());
                            return Ok(ExprCode { pre_expr_stmts, expr: "((void)0)".to_string() });
                        } else {
                            let return_c_type = match &return_type {
                                Some(t) => self.map_type(t),
                                None => "void".to_string(),
                            };
                            let result_temp = self.new_temp_id("call_result_");
                            pre_expr_stmts.push(format!("{return_c_type} {result_temp} = {instantiated_c_identifier}({arguments_code});"));
                            pre_expr_stmts.push("delo_call_pop();".to_string());
                            return Ok(ExprCode { pre_expr_stmts, expr: result_temp });
                        }
                    }
                }

                let optional_callee_type = self.types.expr_types.get(&(callee.as_ref() as *const Expr));

                if let Some(TypeExpr::Function { parameter_types, return_type }) = optional_callee_type {
                    let return_c_type = self.map_type(return_type);
                    let is_void = matches!(return_type.as_ref(), TypeExpr::Named { identifier, .. } if identifier.lexeme == "Void");

                    let parameter_c_types: Vec<String> =
                        parameter_types.iter().map(|t| self.map_type(t)).collect();

                    let function_parameter_types = if parameter_c_types.is_empty() {
                        "void*".to_string()
                    } else {
                        format!("void*, {}", parameter_c_types.join(", "))
                    };

                    let closure_temp = self.new_temp_id("closure_");
                    pre_expr_stmts.push(format!("Fn {closure_temp} = {callee_expr};"));

                    let mut call_arguments = Vec::new();
                    call_arguments.push(format!("{closure_temp}.env"));
                    call_arguments.extend(argument_exprs.into_iter());
                    let args_code = call_arguments.join(", ");

                    let call_expr = format!("(({return_c_type} (* )({function_parameter_types}))({closure_temp}.fn))({args_code})");

                    pre_expr_stmts.push("delo_call_push();".to_string());
                    if is_void {
                        pre_expr_stmts.push(format!("{call_expr};"));
                        pre_expr_stmts.push("delo_call_pop();".to_string());
                        return Ok(ExprCode { pre_expr_stmts, expr: "((void)0)".to_string() });
                    } else {
                        let result_temp = self.new_temp_id("lambda_call_result_");
                        pre_expr_stmts.push(format!("{return_c_type} {result_temp} = {call_expr};"));
                        pre_expr_stmts.push("delo_call_pop();".to_string());
                        return Ok(ExprCode { pre_expr_stmts, expr: result_temp });
                    }
                }
                

                let (line, column) = match &**callee {
                    Expr::Variable(ident) => (ident.line, ident.column),
                    _ => (0, 0),
                };

                Err(CodegenError::InvalidCallTarget {
                    line,
                    column,
                    target_type: optional_callee_type.cloned() 
                })
            }
            Expr::If { condition, then_branch, else_branch, if_token } => {
                let mut pre_expr_stmts = Vec::new();

                let condition_code = self.generate_expr(condition)?;
                pre_expr_stmts.extend(condition_code.pre_expr_stmts);
                let condition_expr = condition_code.expr;

                let result_type = &self.types.expr_types[&(expr as *const Expr)];
                let result_c_type = self.map_type(result_type);
                let result_temp = self.new_temp_id("if_result_");

                pre_expr_stmts.push(format!("{result_c_type} {result_temp};"));

                pre_expr_stmts.push(format!("if ({condition_expr}) {{"));
                self.indent();
                let then_code = self.generate_expr(then_branch)?;
                for line in then_code.pre_expr_stmts {
                    pre_expr_stmts.push(line);
                }
                pre_expr_stmts.push(format!("{result_temp} = {};", then_code.expr));
                self.un_indent();
                pre_expr_stmts.push("} else {".to_string());

                let else_expr = else_branch.as_ref().ok_or(CodegenError::MissingElseInIfExpression {
                    line: if_token.line,
                    column: if_token.column,
                })?;
                self.indent();
                let else_code = self.generate_expr(else_expr)?;
                for line in else_code.pre_expr_stmts {
                    pre_expr_stmts.push(line);
                }
                pre_expr_stmts.push(format!("{result_temp} = {};", else_code.expr));
                self.un_indent();
                pre_expr_stmts.push("}".to_string());

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: result_temp,
                })
            }
            Expr::Match { subject, cases, .. } => {
                let mut pre_expr_stmts = Vec::new();

                let subject_expr = self.generate_expr(subject)?;
                pre_expr_stmts.extend(subject_expr.pre_expr_stmts);
                let subject_expr = subject_expr.expr;

                let subject_type = self.types.expr_types[&(&**subject as *const Expr)].clone();
                let subject_c_type = self.map_type(&subject_type);

                let result_type = self.types.expr_types[&(expr as *const Expr)].clone();
                let result_c_type = self.map_type(&result_type);

                let subject_temp = self.new_temp_id("match_subject_");
                let result_temp = self.new_temp_id("match_result_");

                self.write_indented_to(&mut pre_expr_stmts, self.indent_level, format!("{subject_c_type} {subject_temp} = {subject_expr};"));
                self.write_indented_to(&mut pre_expr_stmts, self.indent_level, format!("{result_c_type} {result_temp};"));

                if is_primitive_c_type(&subject_c_type) {
                    self.write_indented_to(&mut pre_expr_stmts, self.indent_level, "do {".to_string());
                    self.indent();
                    for case in cases {
                        self.append_primitive_match_case(&mut pre_expr_stmts, &subject_c_type, &subject_temp, &result_temp, &subject_type, case)?;
                    }
                    self.un_indent();
                    self.write_indented_to(&mut pre_expr_stmts, self.indent_level, "} while (0);".to_string());
                } else if matches!(subject_type, TypeExpr::Tuple { .. }) {
                    let end_label = self.new_temp_id("match_end_");
                    for case in cases {
                        self.append_tuple_match_case(&mut pre_expr_stmts, &subject_temp, &result_temp, &subject_type, case, &end_label)?;
                    }
                    self.write_indented_to(&mut pre_expr_stmts, self.indent_level, format!("{end_label}:;"));
                } else {
                    self.write_indented_to(&mut pre_expr_stmts, self.indent_level, format!("switch ({subject_temp}.tag) {{"));
                    self.indent();
                    for case in cases {
                        self.append_match_case(&mut pre_expr_stmts, &subject_c_type, &subject_temp, &result_temp, &subject_type, case)?;
                    }
                    self.un_indent();
                    self.write_indented_to(&mut pre_expr_stmts, self.indent_level, "}".to_string());
                }

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: result_temp,
                })
            }
            Expr::Lambda { parameters, return_type, body } => {
                let mut pre_expr_stmts = Vec::new();

                let lambda_base = self.new_temp_id("lambda_");
                let environment_name = format!("{lambda_base}_env");

                let captures = self.types.lambda_captures.get(&(expr as *const Expr)).cloned().unwrap_or_default();

                self.write_line(&format!("typedef struct {environment_name} {{"));
                self.indent();
                self.write_line("char _placeholder;");
                for (identifier, type_expr) in &captures {
                    if self.types.tracked_vars.contains(identifier) {
                        continue;
                    }
                    let field_c_type = self.map_type(type_expr);
                    self.write_line(&format!("{field_c_type} {};", mangle(identifier)));
                }
                self.un_indent();
                self.write_line(&format!("}} {environment_name};"));
                self.write_line("");

                let return_c_type = if let Some(TypeExpr::Function { return_type, .. }) =
                    self.types.expr_types.get(&(expr as *const Expr))
                {
                    self.map_type(return_type)
                } else if let Some(ret_type) = return_type {
                    self.map_type(ret_type)
                } else {
                    "void".to_string()
                };

                let mut parameter_declarations = Vec::new();
                parameter_declarations.push("void* env".to_string());
                for (parameter_identifier, parameter_type) in parameters {
                    let parameter_c_type = self.map_type(parameter_type);
                    parameter_declarations.push(format!("{} {}", parameter_c_type, mangle(&parameter_identifier.lexeme)));
                }
                let parameters_code = parameter_declarations.join(", ");

                self.write_line(&format!("{return_c_type} {lambda_base}({parameters_code}) {{"));
                self.indent();

                for (index, stmt) in body.iter().enumerate() {
                    let is_last = index == body.len() - 1;

                    if return_c_type != "void" && is_last {
                        if let Stmt::Expression(last_expr) = stmt {
                            let last_code = self.generate_expr(last_expr)?;
                            for line in last_code.pre_expr_stmts {
                                self.write_line(&line);
                            }
                            self.write_line(&format!("return {};", last_code.expr));
                            continue;
                        }
                    }

                    let mut lines = Vec::new();
                    self.generate_stmt_to(&mut lines, stmt)?;
                    for line in lines {
                        self.write_line(&line);
                    }
                }

                self.un_indent();
                self.write_line("}");

                let environment_ptr = self.new_temp_id("env_");
                self.write_line("");
                self.write_indented_to(
                    &mut pre_expr_stmts,
                    self.indent_level,
                    format!("{environment_name}* {environment_ptr} = ({environment_name}*)malloc(sizeof({environment_name}));"),
                );

                for (identifier, _) in &captures {
                    if self.types.tracked_vars.contains(identifier) {
                        continue;
                    }
                    let mangled = mangle(identifier);
                    self.write_indented_to(
                        &mut pre_expr_stmts,
                        self.indent_level,
                        format!("{environment_ptr}->{mangled} = {mangled};"),
                    );
                }

                let closure_expr = format!(
                    "(Fn){{ .env = (void*){environment_ptr}, .fn = (void*)&{lambda_base} }}"
                );

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: closure_expr
                })
            }
            Expr::Block { stmts, left_brace } => {
                let mut pre_expr_stmts = Vec::new();

                if stmts.is_empty() {
                    return Err(CodegenError::BlockExpressionMissingEndExpression {
                        line: left_brace.line,
                        column: left_brace.column,
                    });
                }

                for stmt in &stmts[..stmts.len() - 1] {
                    self.generate_stmt_to(&mut pre_expr_stmts, stmt)?;
                }

                match stmts.last().unwrap() {
                    Stmt::Expression(last_expr) => {
                        let last_code = self.generate_expr(last_expr)?;
                        pre_expr_stmts.extend(last_code.pre_expr_stmts);
                        Ok(ExprCode {
                            pre_expr_stmts,
                            expr: last_code.expr,
                        })
                    }
                    _ => {
                        return Err(CodegenError::BlockExpressionMissingEndExpression {
                            line: left_brace.line,
                            column: left_brace.column,
                        });
                    }
                }
            }
            Expr::IndexAccess { target, index, .. } => {
                let mut pre_expr_stmts = Vec::new();

                let target_type = self.types.expr_types[&(target.as_ref() as *const Expr)].clone();

                let target_code = self.generate_expr(target)?;
                pre_expr_stmts.extend(target_code.pre_expr_stmts);
                let target_expr = target_code.expr;

                let index_code = self.generate_expr(index)?;
                pre_expr_stmts.extend(index_code.pre_expr_stmts);
                let index_expr = index_code.expr;

                if let TypeExpr::Named { identifier, type_arguments, .. } = &target_type {
                    if identifier.lexeme == "Map" && type_arguments.len() == 2 {
                        let key_c_type = self.map_type(&type_arguments[0]);
                        let value_c_type = self.map_type(&type_arguments[1]);
                        let key_suffix = UserTypeInstantiation::suffix(&key_c_type);
                        let value_suffix = UserTypeInstantiation::suffix(&value_c_type);
                        let suffix = format!("{key_suffix}_{value_suffix}");
                        self.map_instantiations.insert((key_c_type, value_c_type));
                        let expr_string = format!("map_get_{suffix}({target_expr}, {index_expr})");
                        return Ok(ExprCode { pre_expr_stmts, expr: expr_string });
                    }
                }

                let expr_string = format!("({target_expr}.data[{index_expr}])");

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: expr_string
                })
            }
            Expr::FieldAccess { target, field } => {
                let mut pre_expr_stmts = Vec::new();

                let target_code = self.generate_expr(target)?;
                pre_expr_stmts.extend(target_code.pre_expr_stmts);
                let target_expr = target_code.expr;

                let field_c = if field.token_type == TokenType::Number {
                    format!("_{}", field.lexeme)
                } else {
                    mangle(&field.lexeme)
                };
                let expr_string = format!("({target_expr}.{field_c})");

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: expr_string,
                })
            }
            Expr::TimeTravelAbsolute { target, index, at_token } => {
                let target_name = match target.as_ref() {
                    Expr::Variable(t) => t.lexeme.clone(),
                    _ => return Err(CodegenError::UnexpectedTypeInCodegen {
                        line: at_token.line,
                        column: at_token.column,
                        expected: "variable as time-travel target",
                        found_type: self.types.expr_types[&(target.as_ref() as *const Expr)].clone(),
                    }),
                };
                let var_type = self.types.expr_types[&(expr as *const Expr)].clone();
                let c_type = self.map_type(&var_type);
                let suffix = UserTypeInstantiation::suffix(&c_type);
                self.history_instantiations.insert(c_type.clone());

                let index_code = self.generate_expr(index)?;
                let mangled = mangle(&target_name);
                let expr_string = format!("history_get_{suffix}({mangled}, (size_t)({}))", index_code.expr);
                Ok(ExprCode { pre_expr_stmts: index_code.pre_expr_stmts, expr: expr_string })
            }
            Expr::TimeTravelRelative { target, offset, at_token } => {
                let target_name = match target.as_ref() {
                    Expr::Variable(t) => t.lexeme.clone(),
                    _ => return Err(CodegenError::UnexpectedTypeInCodegen {
                        line: at_token.line,
                        column: at_token.column,
                        expected: "variable as time-travel target",
                        found_type: self.types.expr_types[&(target.as_ref() as *const Expr)].clone(),
                    }),
                };
                let var_type = self.types.expr_types[&(expr as *const Expr)].clone();
                let c_type = self.map_type(&var_type);
                let suffix = UserTypeInstantiation::suffix(&c_type);
                self.history_instantiations.insert(c_type.clone());

                let offset_code = self.generate_expr(offset)?;
                let mangled = mangle(&target_name);
                let expr_string = format!("history_get_{suffix}({mangled}, {mangled}->length - 1 - (size_t)({}))", offset_code.expr);
                Ok(ExprCode { pre_expr_stmts: offset_code.pre_expr_stmts, expr: expr_string })
            }
        }
    }

    fn emit_builtin_call(&mut self, kind: BuiltinCallType, arguments: &[Expr], expr: &Expr) -> Result<ExprCode, CodegenError> {
        match kind {
            BuiltinCallType::ArrayLiteral => {
                let result_type = self.types.expr_types[&(expr as *const Expr)].clone();
                self.emit_array_literal(arguments, &result_type)
            }
            BuiltinCallType::RangeLiteral { is_inclusive } => {
                let result_type = self.types.expr_types[&(expr as *const Expr)].clone();
                self.emit_range_literal(&arguments[0], &arguments[1], is_inclusive, &result_type)
            }
            BuiltinCallType::MapLiteral => {
                let result_type = self.types.expr_types[&(expr as *const Expr)].clone();
                let keys: Vec<&Expr> = arguments.iter().step_by(2).collect();
                let values: Vec<&Expr> = arguments.iter().skip(1).step_by(2).collect();
                self.emit_map_literal(&keys, &values, &result_type)
            }
            BuiltinCallType::Print => self.emit_print(&arguments[0]),
            BuiltinCallType::Map | BuiltinCallType::Filter | BuiltinCallType::Foldl | BuiltinCallType::Foldr => {
                let name = match kind {
                    BuiltinCallType::Map => "map",
                    BuiltinCallType::Filter => "filter",
                    BuiltinCallType::Foldl => "foldl",
                    BuiltinCallType::Foldr => "foldr",
                    _ => unreachable!(),
                };
                self.generate_higher_order_call(name, arguments, expr)
            }
        }
    }

    fn emit_tuple_literal(&mut self, elements: &[Expr], result_type: &TypeExpr) -> Result<ExprCode, CodegenError> {
        if elements.is_empty() {
            return Ok(ExprCode { pre_expr_stmts: Vec::new(), expr: "((void)0)".to_string() });
        }
        let element_types = match result_type {
            TypeExpr::Tuple { element_types } => element_types.clone(),
            _ => {
                return Err(CodegenError::UnexpectedTypeInCodegen {
                    line: 0,
                    column: 0,
                    expected: "tuple literal",
                    found_type: result_type.clone(),
                });
            }
        };

        let mut pre_expr_stmts = Vec::new();
        let mut field_inits = Vec::with_capacity(elements.len());

        for (i, element) in elements.iter().enumerate() {
            let expr_code = self.generate_expr(element)?;
            pre_expr_stmts.extend(expr_code.pre_expr_stmts);
            field_inits.push(format!("._{i} = {}", expr_code.expr));
        }

        let element_c_types: Vec<String> = element_types.iter().map(|t| self.map_type(t)).collect();
        let tuple_name = tuple_c_type_name(&element_c_types);
        self.tuple_instantiations.insert(element_c_types);

        let expr_string = format!("({tuple_name}){{ {} }}", field_inits.join(", "));

        Ok(ExprCode { pre_expr_stmts, expr: expr_string })
    }

    fn emit_array_literal(&mut self, elements: &[Expr], result_type: &TypeExpr) -> Result<ExprCode, CodegenError> {
        let mut pre_expr_stmts = Vec::new();
        let mut element_exprs = Vec::new();

        for element in elements {
            let expr_code = self.generate_expr(element)?;
            pre_expr_stmts.extend(expr_code.pre_expr_stmts);
            element_exprs.push(expr_code.expr);
        }

        let element_type = match result_type {
            TypeExpr::Named { identifier, type_arguments, .. }
                if identifier.lexeme == "Array" && type_arguments.len() == 1 =>
            {
                &type_arguments[0]
            }
            _ => {
                return Err(CodegenError::UnexpectedTypeInCodegen {
                    line: 0,
                    column: 0,
                    expected: "array literal",
                    found_type: result_type.clone(),
                });
            }
        };

        let element_c_type = self.map_type(element_type);
        self.array_instantiations.insert(element_c_type.clone());

        let suffix = UserTypeInstantiation::suffix(&element_c_type);
        let length = elements.len();
        let code = element_exprs.join(", ");
        let expr_string = if length == 0 {
            format!("make_Array_{suffix}(0, ({element_c_type} const*)NULL)")
        } else {
            format!("make_Array_{suffix}({length}, ({element_c_type}[]){{ {code} }})")
        };

        Ok(ExprCode { pre_expr_stmts, expr: expr_string })
    }

    fn emit_map_literal(&mut self, keys: &[&Expr], values: &[&Expr], result_type: &TypeExpr) -> Result<ExprCode, CodegenError> {
        let (key_c_type, value_c_type) = match result_type {
            TypeExpr::Named { identifier, type_arguments, .. }
                if identifier.lexeme == "Map" && type_arguments.len() == 2 =>
            {
                (self.map_type(&type_arguments[0]), self.map_type(&type_arguments[1]))
            }
            _ => {
                return Err(CodegenError::UnexpectedTypeInCodegen {
                    line: 0,
                    column: 0,
                    expected: "map literal type Map<K, V>",
                    found_type: result_type.clone(),
                });
            }
        };
        let key_suffix = UserTypeInstantiation::suffix(&key_c_type);
        let value_suffix = UserTypeInstantiation::suffix(&value_c_type);
        let suffix = format!("{key_suffix}_{value_suffix}");
        self.map_instantiations.insert((key_c_type.clone(), value_c_type.clone()));

        let mut pre_expr_stmts = Vec::new();
        let mut key_exprs = Vec::new();
        let mut value_exprs = Vec::new();
        for (key, value) in keys.iter().zip(values.iter()) {
            let key_code = self.generate_expr(key)?;
            pre_expr_stmts.extend(key_code.pre_expr_stmts);
            key_exprs.push(key_code.expr);
            let value_code = self.generate_expr(value)?;
            pre_expr_stmts.extend(value_code.pre_expr_stmts);
            value_exprs.push(value_code.expr);
        }

        let count = keys.len();
        let expr_string = if count == 0 {
            format!("make_Map_{suffix}(0, ({key_c_type} const*)NULL, ({value_c_type} const*)NULL)")
        } else {
            let keys_array = format!("({key_c_type}[]){{ {} }}", key_exprs.join(", "));
            let values_array = format!("({value_c_type}[]){{ {} }}", value_exprs.join(", "));
            format!("make_Map_{suffix}({count}, {keys_array}, {values_array})")
        };

        Ok(ExprCode { pre_expr_stmts, expr: expr_string })
    }

    fn emit_range_literal(&mut self, start: &Expr, end: &Expr, is_inclusive: bool, result_type: &TypeExpr) -> Result<ExprCode, CodegenError> {
        let mut pre_expr_stmts = Vec::new();

        let start_code = self.generate_expr(start)?;
        pre_expr_stmts.extend(start_code.pre_expr_stmts);
        let start_expr = start_code.expr;

        let end_code = self.generate_expr(end)?;
        pre_expr_stmts.extend(end_code.pre_expr_stmts);
        let end_expr = end_code.expr;

        let inclusive_code = if is_inclusive { "true" } else { "false" };

        let element_type = match result_type {
            TypeExpr::Named { identifier, type_arguments, .. }
                if (identifier.lexeme == "Range" || identifier.lexeme == "InclusiveRange")
                    && type_arguments.len() == 1 =>
            {
                &type_arguments[0]
            }
            _ => {
                return Err(CodegenError::UnexpectedTypeInCodegen {
                    line: 0,
                    column: 0,
                    expected: "range literal (Range or InclusiveRange)",
                    found_type: result_type.clone(),
                });
            }
        };

        let element_c_type = self.map_type(element_type);
        self.range_instantiations.insert(element_c_type.clone());

        let suffix = UserTypeInstantiation::suffix(&element_c_type);
        let expr_string = format!("make_Range_{suffix}({start_expr}, {end_expr}, {inclusive_code})");

        Ok(ExprCode { pre_expr_stmts, expr: expr_string })
    }

    fn emit_print(&mut self, argument: &Expr) -> Result<ExprCode, CodegenError> {
        let argument_code = self.generate_expr(argument)?;
        let argument_type = self.types.expr_types[&(argument as *const Expr)].clone();
        let argument_c_type = self.map_type(&argument_type);
        let helper = self.format_helper_name(&argument_c_type);
        let print_temp = self.new_temp_id("print_arg_");
        let mut pre_expr_stmts = argument_code.pre_expr_stmts;
        pre_expr_stmts.push(format!("{argument_c_type} {print_temp} = {};", argument_code.expr));
        pre_expr_stmts.push("if (!delo_replay_mode) {".to_string());
        pre_expr_stmts.push(format!("    {helper}({print_temp}, stdout);"));
        pre_expr_stmts.push("    putchar('\\n');".to_string());
        pre_expr_stmts.push("}".to_string());
        Ok(ExprCode {
            pre_expr_stmts,
            expr: "((void)0)".to_string(),
        })
    }

    fn format_helper_name(&self, c_type: &str) -> String {
        match c_type {
            "int" => "format_int".to_string(),
            "double" => "format_double".to_string(),
            "bool" => "format_bool".to_string(),
            "const char*" => "format_string".to_string(),
            _ => format!("format_{c_type}"),
        }
    }

    fn generate_higher_order_call(&mut self, name: &str, arguments: &[Expr], expr: &Expr) -> Result<ExprCode, CodegenError> {
        let mut pre_expr_stmts = Vec::new();
        let mut argument_exprs = Vec::new();
        for argument in arguments {
            let argument_code = self.generate_expr(argument)?;
            pre_expr_stmts.extend(argument_code.pre_expr_stmts);
            argument_exprs.push(argument_code.expr);
        }

        let arr_type = self.types.expr_types[&(&arguments[0] as *const Expr)].clone();
        let TypeExpr::Named { type_arguments: arr_type_args, .. } = &arr_type else {
            return Err(CodegenError::UnexpectedTypeInCodegen {
                line: 0,
                column: 0,
                expected: "array argument to higher-order builtin",
                found_type: arr_type.clone(),
            });
        };
        let t_c_type = self.map_type(&arr_type_args[0]);
        let t_suffix = UserTypeInstantiation::suffix(&t_c_type);
        self.array_instantiations.insert(t_c_type.clone());

        match name {
            "map" => {
                let result_type = self.types.expr_types[&(expr as *const Expr)].clone();
                let TypeExpr::Named { type_arguments: result_type_args, .. } = &result_type else {
                    return Err(CodegenError::UnexpectedTypeInCodegen {
                        line: 0,
                        column: 0,
                        expected: "Array result from map",
                        found_type: result_type.clone(),
                    });
                };
                let u_c_type = self.map_type(&result_type_args[0]);
                let u_suffix = UserTypeInstantiation::suffix(&u_c_type);
                self.array_instantiations.insert(u_c_type.clone());
                self.array_map_instantiations.insert((t_c_type, u_c_type));
                let expr_string = format!("array_map_{t_suffix}_{u_suffix}({}, {})", argument_exprs[0], argument_exprs[1]);
                Ok(ExprCode { pre_expr_stmts, expr: expr_string })
            }
            "filter" => {
                self.array_filter_instantiations.insert(t_c_type);
                let expr_string = format!("array_filter_{t_suffix}({}, {})", argument_exprs[0], argument_exprs[1]);
                Ok(ExprCode { pre_expr_stmts, expr: expr_string })
            }
            "foldl" | "foldr" => {
                let result_type = self.types.expr_types[&(expr as *const Expr)].clone();
                let u_c_type = self.map_type(&result_type);
                let u_suffix = UserTypeInstantiation::suffix(&u_c_type);
                let helper = if name == "foldl" {
                    self.array_foldl_instantiations.insert((t_c_type, u_c_type));
                    format!("array_foldl_{t_suffix}_{u_suffix}")
                } else {
                    self.array_foldr_instantiations.insert((t_c_type, u_c_type));
                    format!("array_foldr_{t_suffix}_{u_suffix}")
                };
                let expr_string = format!("{helper}({}, {}, {})", argument_exprs[0], argument_exprs[1], argument_exprs[2]);
                Ok(ExprCode { pre_expr_stmts, expr: expr_string })
            }
            _ => unreachable!(),
        }
    }

    fn append_match_case(&mut self, out: &mut Vec<String>, enum_c_type: &str, subject_temp: &str, result_temp: &str, subject_type: &TypeExpr, case: &MatchCase) -> Result<(), CodegenError> {
        match &case.pattern {
            pattern if self.resolve_zero_ary_variant(subject_type, pattern).is_some() => {
                let variant = self.resolve_zero_ary_variant(subject_type, pattern).unwrap();
                let variant_identifier = mangle(&variant.identifier.lexeme);
                let tag_const = format!("{enum_c_type}_{variant_identifier}");

                self.write_indented_to(out, self.indent_level, format!("case {tag_const}: {{"));
                self.indent();

                if let Some(guard_expr) = &case.guard {
                    let guard_code = self.generate_expr(guard_expr)?;
                    for line in guard_code.pre_expr_stmts {
                        self.write_indented_to(out, self.indent_level, line);
                    }
                    self.write_indented_to(out, self.indent_level, format!("if (!({})) {{ break; }}", guard_code.expr));
                }

                let body_code = self.generate_expr(&case.body)?;
                for line in body_code.pre_expr_stmts {
                    self.write_indented_to(out, self.indent_level, line);
                }
                self.write_indented_to(out, self.indent_level, format!("{result_temp} = {};", body_code.expr));
                self.write_indented_to(out, self.indent_level, "break;".to_string());

                self.un_indent();
                self.write_indented_to(out, self.indent_level, "}".to_string());
            }
            Pattern::EnumVariant { identifier, .. } => {
                let variant_identifier = mangle(&identifier.lexeme);
                let tag_const = format!("{enum_c_type}_{variant_identifier}");
                self.write_indented_to(out, self.indent_level, format!("case {tag_const}: {{"));
                self.indent();

                self.lower_pattern(out, &case.pattern, subject_temp, subject_type)?;

                if let Some(guard_expr) = &case.guard {
                    let guard_code = self.generate_expr(guard_expr)?;
                    for line in guard_code.pre_expr_stmts {
                        self.write_indented_to(out, self.indent_level, line);
                    }
                    self.write_indented_to(out, self.indent_level, format!("if (!({})) {{ break; }}", guard_code.expr));
                }

                let body_code = self.generate_expr(&case.body)?;
                for line in body_code.pre_expr_stmts {
                    self.write_indented_to(out, self.indent_level, line);
                }
                self.write_indented_to(out, self.indent_level, format!("{result_temp} = {};", body_code.expr));
                self.write_indented_to(out, self.indent_level, "break;".to_string());

                self.un_indent();
                self.write_indented_to(out, self.indent_level, "}".to_string());
            }
            Pattern::Variable(identifier) => {
                let var_identifier = mangle(&identifier.lexeme);
                self.write_indented_to(out, self.indent_level, "default: {".to_string());
                self.indent();

                let c_type = self.map_type(subject_type);
                self.write_indented_to(out, self.indent_level, format!("{c_type} {var_identifier} = {subject_temp};"));

                if let Some(guard_expr) = &case.guard {
                    let guard_code = self.generate_expr(guard_expr)?;
                    for line in guard_code.pre_expr_stmts {
                        self.write_indented_to(out, self.indent_level, line);
                    }
                    self.write_indented_to(out, self.indent_level, format!("if (!({})) {{ break; }}", guard_code.expr));
                }

                let body_code = self.generate_expr(&case.body)?;
                for line in body_code.pre_expr_stmts {
                    self.write_indented_to(out, self.indent_level, line);
                }
                self.write_indented_to(out, self.indent_level, format!("{result_temp} = {};", body_code.expr));
                self.write_indented_to(out, self.indent_level, "break;".to_string());

                self.un_indent();
                self.write_indented_to(out, self.indent_level, "}".to_string());
            }
            Pattern::Wildcard(_) => {
                self.write_indented_to(out, self.indent_level, "default: {".to_string());
                self.indent();

                if let Some(guard_expr) = &case.guard {
                    let guard_code = self.generate_expr(guard_expr)?;
                    for line in guard_code.pre_expr_stmts {
                        out.push(line);
                    }
                    self.write_indented_to(out, self.indent_level, format!("if (!({})) {{ break; }}", guard_code.expr));
                }

                let body_code = self.generate_expr(&case.body)?;
                for line in body_code.pre_expr_stmts {
                    self.write_indented_to(out, self.indent_level, line);
                }
                self.write_indented_to(out, self.indent_level, format!("{result_temp} = {};", body_code.expr));
                self.write_indented_to(out, self.indent_level, "break;".to_string());

                self.un_indent();
                self.write_indented_to(out, self.indent_level, "}".to_string());
            }
            Pattern::Literal(literal) => {
                return Err(CodegenError::UnsupportedMatchPattern {
                    line: literal.line,
                    column: literal.column,
                })
            }
            Pattern::Range { start, .. } => {
                return Err(CodegenError::UnsupportedMatchPattern {
                    line: start.line,
                    column: start.column,
                })
            }
            Pattern::Tuple { left_paren, .. } => {
                return Err(CodegenError::UnsupportedMatchPattern {
                    line: left_paren.line,
                    column: left_paren.column,
                })
            }
        }

        Ok(())
    }

    fn append_tuple_match_case(
        &mut self,
        out: &mut Vec<String>,
        subject_temp: &str,
        result_temp: &str,
        subject_type: &TypeExpr,
        case: &MatchCase,
        end_label: &str,
    ) -> Result<(), CodegenError> {
        self.write_indented_to(out, self.indent_level, "do {".to_string());
        self.indent();

        self.lower_pattern(out, &case.pattern, subject_temp, subject_type)?;

        if let Some(guard_expr) = &case.guard {
            let guard_code = self.generate_expr(guard_expr)?;
            for line in guard_code.pre_expr_stmts {
                self.write_indented_to(out, self.indent_level, line);
            }
            self.write_indented_to(out, self.indent_level, format!("if (!({})) {{ break; }}", guard_code.expr));
        }

        let body_code = self.generate_expr(&case.body)?;
        for line in body_code.pre_expr_stmts {
            self.write_indented_to(out, self.indent_level, line);
        }
        self.write_indented_to(out, self.indent_level, format!("{result_temp} = {};", body_code.expr));
        self.write_indented_to(out, self.indent_level, format!("goto {end_label};"));

        self.un_indent();
        self.write_indented_to(out, self.indent_level, "} while (0);".to_string());

        Ok(())
    }

    fn append_primitive_match_case(
        &mut self,
        out: &mut Vec<String>,
        subject_c_type: &str,
        subject_temp: &str,
        result_temp: &str,
        subject_type: &TypeExpr,
        case: &MatchCase,
    ) -> Result<(), CodegenError> {
        let kind = self.classify_primitive_pattern(&case.pattern, subject_type, subject_temp, subject_c_type)?;

        match kind {
            PrimitivePatternType::CatchAll { binding } => {
                self.write_indented_to(out, self.indent_level, "{".to_string());
                self.indent();
                if let Some((var_name, var_c_type)) = binding {
                    self.write_indented_to(out, self.indent_level, format!("{var_c_type} {var_name} = {subject_temp};"));
                }
                self.emit_match_arm_body(out, case, result_temp)?;
                self.un_indent();
                self.write_indented_to(out, self.indent_level, "}".to_string());
            }
            PrimitivePatternType::Conditional { condition } => {
                self.write_indented_to(out, self.indent_level, format!("if ({condition}) {{"));
                self.indent();
                self.emit_match_arm_body(out, case, result_temp)?;
                self.un_indent();
                self.write_indented_to(out, self.indent_level, "}".to_string());
            }
        }

        Ok(())
    }

    fn emit_match_arm_body(&mut self, out: &mut Vec<String>, case: &MatchCase, result_temp: &str) -> Result<(), CodegenError> {
        if let Some(guard_expr) = &case.guard {
            let guard_code = self.generate_expr(guard_expr)?;
            for line in guard_code.pre_expr_stmts {
                self.write_indented_to(out, self.indent_level, line);
            }
            self.write_indented_to(out, self.indent_level, format!("if ({}) {{", guard_code.expr));
            self.indent();
            let body_code = self.generate_expr(&case.body)?;
            for line in body_code.pre_expr_stmts {
                self.write_indented_to(out, self.indent_level, line);
            }
            self.write_indented_to(out, self.indent_level, format!("{result_temp} = {};", body_code.expr));
            self.write_indented_to(out, self.indent_level, "break;".to_string());
            self.un_indent();
            self.write_indented_to(out, self.indent_level, "}".to_string());
        } else {
            let body_code = self.generate_expr(&case.body)?;
            for line in body_code.pre_expr_stmts {
                self.write_indented_to(out, self.indent_level, line);
            }
            self.write_indented_to(out, self.indent_level, format!("{result_temp} = {};", body_code.expr));
            self.write_indented_to(out, self.indent_level, "break;".to_string());
        }
        Ok(())
    }

    fn classify_primitive_pattern(
        &self,
        pattern: &Pattern,
        subject_type: &TypeExpr,
        subject_temp: &str,
        subject_c_type: &str,
    ) -> Result<PrimitivePatternType, CodegenError> {
        match pattern {
            Pattern::Wildcard(_) => Ok(PrimitivePatternType::CatchAll { binding: None }),
            Pattern::Variable(identifier) => {
                if subject_c_type == "bool" {
                    match identifier.lexeme.as_str() {
                        "True" => return Ok(PrimitivePatternType::Conditional {
                            condition: format!("{subject_temp} == true"),
                        }),
                        "False" => return Ok(PrimitivePatternType::Conditional {
                            condition: format!("{subject_temp} == false"),
                        }),
                        _ => {}
                    }
                }
                Ok(PrimitivePatternType::CatchAll {
                    binding: Some((mangle(&identifier.lexeme), subject_c_type.to_string())),
                })
            }
            Pattern::Literal(literal) => {
                let lit_lexeme = &literal.lexeme;
                let condition = if subject_c_type == "const char*" {
                    format!("strcmp({subject_temp}, {lit_lexeme}) == 0")
                } else {
                    format!("{subject_temp} == {lit_lexeme}")
                };
                Ok(PrimitivePatternType::Conditional { condition })
            }
            Pattern::Range { start, end, is_inclusive } => {
                let start_lexeme = &start.lexeme;
                let end_lexeme = &end.lexeme;
                let condition = if subject_c_type == "const char*" {
                    if *is_inclusive {
                        format!("strcmp({subject_temp}, {start_lexeme}) >= 0 && strcmp({subject_temp}, {end_lexeme}) <= 0")
                    } else {
                        format!("strcmp({subject_temp}, {start_lexeme}) >= 0 && strcmp({subject_temp}, {end_lexeme}) < 0")
                    }
                } else if *is_inclusive {
                    format!("{subject_temp} >= {start_lexeme} && {subject_temp} <= {end_lexeme}")
                } else {
                    format!("{subject_temp} >= {start_lexeme} && {subject_temp} < {end_lexeme}")
                };
                Ok(PrimitivePatternType::Conditional { condition })
            }
            Pattern::EnumVariant { identifier, .. } => {
                Err(CodegenError::UnexpectedTypeInCodegen {
                    line: identifier.line,
                    column: identifier.column,
                    expected: "primitive match pattern (literal, range, wildcard, or variable)",
                    found_type: subject_type.clone(),
                })
            }
            Pattern::Tuple { left_paren, .. } => {
                Err(CodegenError::UnexpectedTypeInCodegen {
                    line: left_paren.line,
                    column: left_paren.column,
                    expected: "primitive match pattern (literal, range, wildcard, or variable)",
                    found_type: subject_type.clone(),
                })
            }
        }
    }

    fn resolve_zero_ary_variant(&self, subject_type: &TypeExpr, pattern: &Pattern) -> Option<EnumVariant> {
        let variant_identifier = match pattern {
            Pattern::EnumVariant { identifier, arguments } => {
                if !arguments.is_empty() {
                    return None;
                }
                &identifier.lexeme
            }
            Pattern::Variable(identifier) => &identifier.lexeme,
            _ => return None,
        };

        let TypeExpr::Named { enum_variants, .. } = subject_type else {
            return None;
        };

        let variants = enum_variants.as_ref()?;

        variants.iter().find(|v| v.identifier.lexeme == *variant_identifier && v.payload_types.is_empty()).cloned()
    }

    fn lower_pattern(&mut self, out: &mut Vec<String>, pattern: &Pattern, subject_expr: &str, subject_type: &TypeExpr) -> Result<(), CodegenError> {
        match pattern {
            Pattern::Wildcard(_) => {}
            Pattern::Variable(identifier) => {
                let var_identifier = mangle(&identifier.lexeme);
                let c_type = self.map_type(subject_type);
                self.write_indented_to(out, self.indent_level, format!("{c_type} {var_identifier} = {subject_expr};"));
            }
            Pattern::Literal(literal) => {
                let literal_expr = literal.lexeme.clone();
                let c_type = self.map_type(subject_type);
                let check = if c_type == "const char*" {
                    format!("if (strcmp({subject_expr}, {literal_expr}) != 0) {{ break; }}")
                } else {
                    format!("if ({subject_expr} != {literal_expr}) {{ break; }}")
                };
                self.write_indented_to(out, self.indent_level, check);
            }
            Pattern::EnumVariant { identifier, arguments } => {
                let TypeExpr::Named { enum_variants, .. } = subject_type else {
                    return Err(CodegenError::UnexpectedTypeInCodegen {
                        line: identifier.line,
                        column: identifier.column,
                        expected: "enum pattern subject",
                        found_type: subject_type.clone(),
                    });
                };
                let variants = enum_variants.as_ref().ok_or(CodegenError::MissingType { 
                    line: identifier.line, 
                    column: identifier.column, 
                    identifier: identifier.lexeme.clone() 
                })?;
                let variant = variants.iter().find(|v| v.identifier.lexeme == identifier.lexeme).ok_or(CodegenError::UnknownEnumVariantInPattern {
                    line: identifier.line,
                    column: identifier.column,
                    enum_identifier: identifier.lexeme.clone(),
                })?;

                let instantiated_variant = self.instantiate_enum_variant_payloads(subject_type, variant);

                if arguments.len() != instantiated_variant.payload_types.len() {
                    return Err(CodegenError::InvalidEnumPatternArgumentCount {
                        line: identifier.line,
                        column: identifier.column,
                        enum_identifier: identifier.lexeme.clone(),
                        variant_identifier: instantiated_variant.identifier.lexeme.clone(),
                        expected: instantiated_variant.payload_types.len(),
                        found: arguments.len(),
                    });
                }

                for (index, (argument_pattern, payload_type)) in arguments.iter().zip(instantiated_variant.payload_types.iter()).enumerate() {
                    let field_identifier = if instantiated_variant.payload_types.len() == 1 {
                        "value".to_string()
                    } else {
                        format!("field{index}")
                    };

                    let payload_expr = format!("{}.data.{}.{}", subject_expr, mangle(&identifier.lexeme), field_identifier);

                    self.lower_pattern(out, argument_pattern, &payload_expr, &payload_type)?;
                }
            }
            Pattern::Range { start, end, is_inclusive } => {
                let start_expr = start.lexeme.clone();
                let end_expr = end.lexeme.clone();
                let c_type = self.map_type(subject_type);
                let check = if c_type == "const char*" {
                    if *is_inclusive {
                        format!("if (strcmp({subject_expr}, {start_expr}) < 0 || strcmp({subject_expr}, {end_expr}) > 0) {{ break; }}")
                    } else {
                        format!("if (strcmp({subject_expr}, {start_expr}) < 0 || strcmp({subject_expr}, {end_expr}) >= 0) {{ break; }}")
                    }
                } else if *is_inclusive {
                    format!("if ({subject_expr} < {start_expr} || {subject_expr} > {end_expr}) {{ break; }}")
                } else {
                    format!("if ({subject_expr} < {start_expr} || {subject_expr} >= {end_expr}) {{ break; }}")
                };
                self.write_indented_to(out, self.indent_level, check);
            }
            Pattern::Tuple { elements, left_paren } => {
                let element_types = match subject_type {
                    TypeExpr::Tuple { element_types } => element_types.clone(),
                    other => return Err(CodegenError::UnexpectedTypeInCodegen {
                        line: left_paren.line,
                        column: left_paren.column,
                        expected: "tuple type for tuple pattern",
                        found_type: other.clone(),
                    }),
                };

                for (i, sub_pattern) in elements.iter().enumerate() {
                    let sub_subject = format!("{subject_expr}._{i}");
                    self.lower_pattern(out, sub_pattern, &sub_subject, &element_types[i])?;
                }
            }
        }

        Ok(())
    }

    fn instantiate_enum_variant_payloads(&self, subject_type: &TypeExpr, variant: &EnumVariant) -> EnumVariant {
        let TypeExpr::Named { type_parameters, type_arguments, .. } = subject_type else {
            return variant.clone();
        };

        let type_parameters = match type_parameters {
            Some(parameters) => parameters,
            None => return variant.clone()
        };

        let mut mapping: HashMap<usize, TypeExpr> = HashMap::new();
        for (parameter, argument) in type_parameters.iter().zip(type_arguments.iter()) {
            if let TypeExpr::TypeVar { id } = parameter {
                mapping.insert(*id, argument.clone());
            }
        }

        let new_payloads = variant.payload_types.iter().map(|t| self.substitute_local(t, &mapping)).collect();

        EnumVariant {
            identifier: variant.identifier.clone(),
            payload_types: new_payloads,
        }
    }

    fn substitute_local(&self, type_expr: &TypeExpr, mapping: &HashMap<usize, TypeExpr>) -> TypeExpr {
        match type_expr {
            TypeExpr::TypeVar { id } => mapping.get(id).cloned().unwrap_or_else(|| type_expr.clone()),
            TypeExpr::Named { identifier, type_parameters, type_arguments, enum_variants, struct_fields } => TypeExpr::Named {
                identifier: identifier.clone(),
                type_parameters: type_parameters.clone(),
                type_arguments: type_arguments.iter().map(|t| self.substitute_local(t, mapping)).collect(),
                enum_variants: enum_variants.clone(),
                struct_fields: struct_fields.clone(),
            },
            TypeExpr::Function { parameter_types, return_type } => TypeExpr::Function {
                parameter_types: parameter_types.iter().map(|t| self.substitute_local(t, mapping)).collect(),
                return_type: Box::new(self.substitute_local(return_type, mapping)),
            },
            TypeExpr::Tuple { element_types } => TypeExpr::Tuple {
                element_types: element_types.iter().map(|t| self.substitute_local(t, mapping)).collect(),
            },
        }
    }

    fn generate_declarations(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Struct { identifier, type_parameters, fields } => {
                self.ast_structs.insert(identifier.lexeme.clone(), stmt.clone());

                if type_parameters.is_empty() {
                    self.generate_struct_declaration(identifier, fields);
                }
            }
            Stmt::Enum { identifier, type_parameters, variants } => {
                self.ast_enums.insert(identifier.lexeme.clone(), stmt.clone());

                if type_parameters.is_empty() {
                    let c_id = mangle(&identifier.lexeme);
                    let instantiation = UserTypeInstantiation {
                        identifier: identifier.lexeme.clone(),
                        argument_c_types: Vec::new(),
                    };
                    self.write_enum_instantiation(&c_id, &Vec::new(), variants, &instantiation);
                }
            }
            Stmt::Function(function) => {
                self.ast_functions.insert(function.identifier.lexeme.clone(), function.clone());
            }
            _ => {}
        }
    }

    fn generate_struct_declaration(&mut self, identifier: &Token, fields: &Vec<(Token, TypeExpr)>) {
        let mangled = mangle(&identifier.lexeme);
        self.write_line(&format!("typedef struct {mangled} {{"));
        self.indent();
        for (field_identifier, field_type) in fields {
            let c_field_type = self.map_type(field_type);
            self.write_line(&format!("{} {};", c_field_type, mangle(&field_identifier.lexeme)))
        }
        self.un_indent();
        self.write_line(&format!("}} {mangled};"));
        self.write_line("");
    }

    fn emit_variable_binding(&mut self, out: &mut Vec<String>, binding: &VariableBinding, delo_type: &TypeExpr, c_type: &str, init_code: &str) -> Result<(), CodegenError> {
        match binding {
            VariableBinding::Identifier(token) => {
                self.write_indented_to(out, self.indent_level, format!("{} {} = {};", c_type, mangle(&token.lexeme), init_code));
                Ok(())
            }
            VariableBinding::Tuple { elements, left_paren } => {
                let element_types = match delo_type {
                    TypeExpr::Tuple { element_types } => element_types.clone(),
                    other => return Err(CodegenError::UnexpectedTypeInCodegen {
                        line: left_paren.line,
                        column: left_paren.column,
                        expected: "tuple type for tuple destructure binding",
                        found_type: other.clone(),
                    }),
                };

                let temp = self.new_temp_id("dest_");
                self.write_indented_to(out, self.indent_level, format!("{} {} = {};", c_type, temp, init_code));

                for (i, sub_binding) in elements.iter().enumerate() {
                    let sub_delo_type = element_types[i].clone();
                    let sub_c_type = self.map_type(&sub_delo_type);
                    let sub_init = format!("{}._{}", temp, i);
                    self.emit_variable_binding(out, sub_binding, &sub_delo_type, &sub_c_type, &sub_init)?;
                }
                Ok(())
            }
        }
    }

    fn map_type(&mut self, type_expr: &TypeExpr) -> String {
        match type_expr {
            TypeExpr::Named { identifier, type_arguments, .. } => {
                let c_identifier = match identifier.lexeme.as_str() {
                    "Int" => "int".to_string(),
                    "Double" => "double".to_string(),
                    "String" => "const char*".to_string(),
                    "Bool" => "bool".to_string(),
                    "Void" => "void".to_string(),
                    "Array" if type_arguments.len() == 1 => {
                        let c_element = self.map_type(&type_arguments[0]);
                        let suffix = UserTypeInstantiation::suffix(&c_element);
                        self.array_instantiations.insert(c_element.clone());
                        format!("Array_{suffix}")
                    }
                    "Map" if type_arguments.len() == 2 => {
                        let key_c_type = self.map_type(&type_arguments[0]);
                        let value_c_type = self.map_type(&type_arguments[1]);
                        let key_suffix = UserTypeInstantiation::suffix(&key_c_type);
                        let value_suffix = UserTypeInstantiation::suffix(&value_c_type);
                        self.map_instantiations.insert((key_c_type.clone(), value_c_type.clone()));
                        format!("Map_{key_suffix}_{value_suffix}")
                    }
                    "Range" | "InclusiveRange" if type_arguments.len() == 1 => {
                        let c_bound = self.map_type(&type_arguments[0]);
                        let suffix = UserTypeInstantiation::suffix(&c_bound);
                        self.range_instantiations.insert(c_bound.clone());
                        format!("Range_{suffix}")
                    }

                    _ => {
                        if type_arguments.is_empty() {
                            mangle(&identifier.lexeme)
                        } else {
                            let argument_c_types = type_arguments.iter().map(|t| self.map_type(t)).collect();
                            let instantiation = UserTypeInstantiation { identifier: identifier.lexeme.clone(), argument_c_types };
                            let c_identifier = instantiation.c_identifier();
                            self.user_type_instantiations.insert(instantiation);

                            c_identifier
                        }
                    }
                };

                c_identifier
            }
            TypeExpr::Function { .. } => "Fn".to_string(),
            TypeExpr::Tuple { element_types } => {
                let element_c_types: Vec<String> = element_types.iter().map(|t| self.map_type(t)).collect();
                let name = tuple_c_type_name(&element_c_types);
                self.tuple_instantiations.insert(element_c_types);
                name
            }
            TypeExpr::TypeVar { .. } => "void*".to_string()
        }
    }

    fn map_type_with_parameters(&mut self, type_expr: &TypeExpr, bindings: &[(String, String)]) -> String {
        match type_expr {
            TypeExpr::Named { identifier, type_arguments, .. } => {
                if type_arguments.is_empty() {
                    if let Some((_, c_type)) = bindings.iter().find(|(parameter_identifier, _)| parameter_identifier == &identifier.lexeme) {
                        return c_type.clone();
                    }
                }
                self.map_type(type_expr)
            }
            _ => self.map_type(type_expr),
        }
    }

    fn new_temp_id(&mut self, prefix: &str) -> String {
        let id = self.temp_id_counter;
        self.temp_id_counter += 1;
        format!("__{prefix}{id}")
    }

    fn write_line(&mut self, line: &str) {
        for _ in 0..self.indent_level {
            self.output.push_str("    ");
        }
        self.output.push_str(line);
        self.output.push_str("\n");
    }

    // start content with \n for dedented multi-line strings based on smallest non-blank indent
    fn write_raw(&mut self, content: &str) {
        let Some(stripped) = content.strip_prefix('\n') else {
            self.output.push_str(content);
            return;
        };

        let min_indent = stripped
            .split('\n')
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.bytes().take_while(|b| *b == b' ').count())
            .min()
            .unwrap_or(0);

        for (i, line) in stripped.split('\n').enumerate() {
            if i > 0 {
                self.output.push('\n');
            }
            if line.trim().is_empty() {
                continue;
            }
            if min_indent > 0 && line.len() >= min_indent && line.bytes().take(min_indent).all(|b| b == b' ') {
                self.output.push_str(&line[min_indent..]);
            } else {
                self.output.push_str(line);
            }
        }
    }

    fn write_indented_to(&mut self, out: &mut Vec<String>, indent: usize, text: String) {
        let mut string = String::new();
        for _ in 0..indent {
            string.push_str("    ");
        }
        string.push_str(&text);
        out.push(string);
    }
    
    fn indent(&mut self) {
        self.indent_level += 1;
    }

    fn un_indent(&mut self) {
        self.indent_level = self.indent_level.saturating_sub(1);
    }
}