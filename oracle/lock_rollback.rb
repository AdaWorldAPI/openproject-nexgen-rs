# Oracle for rails-polyfill's
# `a_rolled_back_locked_save_leaves_the_record_stale_as_rails_does`.
#
# What ActiveRecord does to an optimistically locked record when an
# after_save callback raises after its UPDATE succeeded. Runs ActiveRecord
# from the OpenProject checkout's vendor/bundle, unchanged, against an
# in-memory SQLite database (the sqlite3 gem is the only extra):
#
#   GEM_HOME=<dir> gem install sqlite3
#   OPENPROJECT_SRC=<checkout> GEM_HOME=<dir> GEM_PATH=<dir> ruby oracle/lock_rollback.rb
#
# Output recorded with ActiveRecord 8.1.4.1 (OpenProject 2725194):
#   ActiveRecord 8.1.4.1
#   save raised: boom
#   after rollback: record lock_version=1 changed=["subject", "lock_version"] stored=0
#   retry: StaleObjectError
#   create rolled back: new_record=true id=nil rows=1
#   create retry: saved new_record=false rows=2
#   destroy returned: false
#   destroy rolled back: record name=b changed=["name"] stored=a destroyed=false
#   nested: outer save true; children=0 parents=1 outers=1
#   nested: ["parent destroy returned false", "child after_commit destroy"]
#   rescued: kids=0 moms=0 ["rescued kid boom", "mom after_commit destroy", "kid after_commit destroy"]
#   save then destroy: solos=0 ["after_commit any", "after_commit destroy"]
#   twice: tots=0 ["first Tot", "second Tot", "tot after_commit destroy"]
#   create then destroy: blips=0 destroyed=true ["blip after_commit any", "blip after_commit destroy"]
#   rescued save: cubs=1 ["rescued cub boom", "cub after_commit c"]
#   loop creates: pups=2 ["pup after_commit create p1", "pup after_commit create p2"]
#   same row twice (first-saved=true): ["kit after_commit k1"]
#   same row twice (first-saved=false): ["kit after_commit k2"]
#   destroyed new save: returned=false rows=0 new_record=true ["before_validation", "before_save"]
#   destroyed persisted save: returned=false rows=0 ["before_validation", "before_save"]
B = File.join(ENV.fetch("OPENPROJECT_SRC"), "vendor/bundle/ruby/4.0.0/gems")
%w[activerecord-8.1.4.1 activemodel-8.1.4.1 activesupport-8.1.4.1].each { |g| $LOAD_PATH.unshift("#{B}/#{g}/lib") }
Dir["#{B}/{concurrent-ruby,i18n,tzinfo,minitest,base64,bigdecimal,connection_pool,drb,logger,securerandom,uri,benchmark,timeout,json}-*/lib"].each { |d| $LOAD_PATH.unshift(d) }
Dir["#{B}/concurrent-ruby-*/lib/concurrent-ruby"].each { |d| $LOAD_PATH.unshift(d) }
require "active_record"
puts "ActiveRecord #{ActiveRecord.version}"
ActiveRecord::Base.establish_connection(adapter: "sqlite3", database: ":memory:")
ActiveRecord::Schema.verbose = false
ActiveRecord::Schema.define do
  create_table(:wps) { |t| t.string :subject; t.integer :lock_version, default: 0, null: false }
  create_table(:plains) { |t| t.string :name }
  create_table(:children) { |t| t.string :name }
  create_table(:parents) { |t| t.string :name }
  create_table(:outers) { |t| t.string :name }
  %i[kids moms solos tots holders blips cubs dens pups litters].each { |t| create_table(t) { |c| c.string :name } }
end
class Wp < ActiveRecord::Base
  attr_accessor :boom
  after_save { raise "boom" if boom }
end
w = Wp.create!(subject: "a")
w.subject = "b"; w.boom = true
begin; w.save; rescue => e; puts "save raised: #{e.message}"; end
puts "after rollback: record lock_version=#{w.lock_version} changed=#{w.changed.inspect} stored=#{Wp.find(w.id).lock_version}"
w.boom = false; w.subject = "c"
begin
  w.save!; puts "retry: saved, stored lock_version=#{Wp.find(w.id).lock_version}"
rescue ActiveRecord::StaleObjectError
  puts "retry: StaleObjectError"
end
n = Wp.new(subject: "n"); n.boom = true
begin; n.save; rescue => e; end
puts "create rolled back: new_record=#{n.new_record?} id=#{n.id.inspect} rows=#{Wp.count}"
n.boom = false
n.save!
puts "create retry: saved new_record=#{n.new_record?} rows=#{Wp.count}"

# A before_destroy that saves the record, then aborts.
class Plain < ActiveRecord::Base
  before_destroy { save!; throw :abort }
end
r = Plain.create!(name: "a")
r.name = "b"
puts "destroy returned: #{r.destroy.inspect}"
puts "destroy rolled back: record name=#{r.name} changed=#{r.changed.inspect} stored=#{Plain.find(r.id).name} destroyed=#{r.destroyed?}"

# A destroy nested in an outer save: the parent's before_destroy destroys a
# child, then aborts. The nested transaction has no savepoint.
$log = []
class Child < ActiveRecord::Base
  after_commit(on: :destroy) { $log << "child after_commit destroy" }
end
class Parent < ActiveRecord::Base
  before_destroy { Child.first.destroy; throw :abort }
end
class Outer < ActiveRecord::Base
  after_save { $log << "parent destroy returned #{Parent.first.destroy.inspect}" }
end
Child.create!(name: "c"); Parent.create!(name: "p")
o = Outer.new(name: "o")
saved = o.save
puts "nested: outer save #{saved.inspect}; children=#{Child.count} parents=#{Parent.count} outers=#{Outer.count}"
puts "nested: #{$log.inspect}"

# A nested destroy whose after_destroy raises, rescued by the outer
# before_destroy.
$log = []
class Kid < ActiveRecord::Base
  after_destroy { raise "kid boom" }
  after_commit(on: :destroy) { $log << "kid after_commit destroy" }
end
class Mom < ActiveRecord::Base
  before_destroy { begin; Kid.first.destroy; rescue => e; $log << "rescued #{e.message}"; end }
  after_commit(on: :destroy) { $log << "mom after_commit destroy" }
end
Kid.create!(name: "k"); mom = Mom.create!(name: "m")
mom.destroy
puts "rescued: kids=#{Kid.count} moms=#{Mom.count} #{$log.inspect}"

# A before_destroy that saves the same record, then the destroy proceeds.
class Solo < ActiveRecord::Base
  before_destroy { self.name = "changed"; save! }
  after_commit(on: :update) { $log << "after_commit update" }
  after_commit(on: :destroy) { $log << "after_commit destroy" }
  after_commit { $log << "after_commit any" }
end
solo = Solo.create!(name: "s")
$log = []
solo.destroy
puts "save then destroy: solos=#{Solo.count} #{$log.inspect}"

# One record destroyed twice inside an outer save.
$log = []
class Tot < ActiveRecord::Base
  after_commit(on: :destroy) { $log << "tot after_commit destroy" }
end
class Holder < ActiveRecord::Base
  after_save { t = Tot.first; $log << "first #{t.destroy.class}"; $log << "second #{t.destroy.class}" }
end
Tot.create!(name: "t")
Holder.create!(name: "h")
puts "twice: tots=#{Tot.count} #{$log.inspect}"

# An after_create that destroys the new record.
$log = []
class Blip < ActiveRecord::Base
  after_create { destroy }
  after_commit(on: :create) { $log << "blip after_commit create" }
  after_commit(on: :destroy) { $log << "blip after_commit destroy" }
  after_commit { $log << "blip after_commit any" }
end
b = Blip.create!(name: "b")
puts "create then destroy: blips=#{Blip.count} destroyed=#{b.destroyed?} #{$log.inspect}"

# A nested create whose after_save raises, rescued by the outer callback.
$log = []
class Cub < ActiveRecord::Base
  after_save { raise "cub boom" }
  after_commit { $log << "cub after_commit #{name}" }
end
class Den < ActiveRecord::Base
  after_save { begin; Cub.create!(name: "c"); rescue => e; $log << "rescued #{e.message}"; end }
end
Den.create!(name: "d")
puts "rescued save: cubs=#{Cub.count} #{$log.inspect}"

# Two records created one after the other in a loop.
$log = []
class Pup < ActiveRecord::Base
  after_commit(on: :create) { $log << "pup after_commit create #{name}" }
end
class Litter < ActiveRecord::Base
  after_save { %w[p1 p2].each { |n| Pup.create!(name: n) } }
end
Litter.create!(name: "l")
puts "loop creates: pups=#{Pup.count} #{$log.inspect}"

# Two instances of one existing row, both saved in one transaction.
# ActiveRecord picks one callback recipient per equal record (same class
# and id); with run_commit_callbacks_on_first_saved_instances_in_transaction
# (default true, and OpenProject's effective value under load_defaults 6.1)
# it is the first instance saved.
ActiveRecord::Schema.define do
  create_table(:kits) { |t| t.string :name }
  create_table(:shelves) { |t| t.string :name }
end
$log = []
class Kit < ActiveRecord::Base
  after_commit { $log << "kit after_commit #{name}" }
end
class Shelf < ActiveRecord::Base
  after_save do
    a = Kit.find($kit_id); b = Kit.find($kit_id)
    a.update!(name: "k1"); b.update!(name: "k2")
  end
end
$kit_id = Kit.create!(name: "k0").id
$log = []
Shelf.create!(name: "s")
puts "same row twice (first-saved=#{Kit.run_commit_callbacks_on_first_saved_instances_in_transaction}): #{$log.inspect}"
Kit.run_commit_callbacks_on_first_saved_instances_in_transaction = false
$log = []
Shelf.create!(name: "s2")
puts "same row twice (first-saved=false): #{$log.inspect}"

# A destroyed record is not saved: `create_or_update` returns false when
# `destroyed?` (persistence.rb:925). It runs inside `_run_save_callbacks`
# (callbacks.rb:439-441), after validation, so the validation and before_save
# callbacks still run; the write and the create/update callbacks do not.
ActiveRecord::Schema.define { create_table(:gones) { |t| t.string :name } }
class Gone < ActiveRecord::Base
  before_validation { $log << "before_validation" }
  before_save { $log << "before_save" }
  after_save { $log << "after_save" }
  after_create { $log << "after_create" }
  after_update { $log << "after_update" }
  after_commit { $log << "after_commit" }
  after_rollback { $log << "after_rollback" }
end
$log = []
fresh = Gone.new(name: "g")
fresh.destroy
r = fresh.save
puts "destroyed new save: returned=#{r} rows=#{Gone.count} new_record=#{fresh.new_record?} #{$log.inspect}"
kept = Gone.create!(name: "k")
kept.destroy
$log = []
r = kept.save
puts "destroyed persisted save: returned=#{r} rows=#{Gone.count} #{$log.inspect}"
